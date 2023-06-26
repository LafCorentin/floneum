#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")] // hide console window on Windows in release

use eframe::egui::{DragValue, Visuals};
use eframe::epaint::Vec2;
use eframe::{
    egui::{self, TextEdit},
    epaint::ahash::{HashMap, HashSet},
};
use egui_node_graph::*;
use floneum_plugin::exports::plugins::main::definitions::{
    Embedding, PrimitiveValue, PrimitiveValueType, Value, ValueType,
};
use floneum_plugin::plugins::main::types::{
    EmbeddingDbId, GptNeoXType, LlamaType, ModelId, ModelType, MptType,
};
use floneum_plugin::{Plugin, PluginEngine, PluginInstance};
use floneumate::Index;
use log::LevelFilter;
use once_cell::sync::Lazy;
use pollster::FutureExt;
use serde::{de::DeserializeOwned, Deserialize, Serialize};
use std::{
    borrow::Cow,
    fmt::{Debug, Formatter},
    fs::File,
    io::Read,
    io::Write,
    path::PathBuf,
};
use tokio::sync::mpsc::{Receiver, Sender};

trait Variants: Sized + 'static {
    const VARIANTS: &'static [Self];
}

impl Variants for ModelType {
    const VARIANTS: &'static [Self] = &[
        ModelType::Llama(LlamaType::Guanaco),
        ModelType::Llama(LlamaType::Orca),
        ModelType::Llama(LlamaType::Vicuna),
        ModelType::Llama(LlamaType::Wizardlm),
        ModelType::GptNeoX(GptNeoXType::TinyPythia),
        ModelType::GptNeoX(GptNeoXType::LargePythia),
        ModelType::GptNeoX(GptNeoXType::Stablelm),
        ModelType::GptNeoX(GptNeoXType::DollySevenB),
        ModelType::Mpt(MptType::Base),
        ModelType::Mpt(MptType::Chat),
        ModelType::Mpt(MptType::Story),
        ModelType::Mpt(MptType::Instruct),
    ];
}

trait Named {
    fn name(&self) -> &'static str;
}

impl Named for ModelType {
    fn name(&self) -> &'static str {
        match self {
            ModelType::Llama(LlamaType::Guanaco) => "Guanaco",
            ModelType::Llama(LlamaType::Orca) => "Orca",
            ModelType::Llama(LlamaType::Vicuna) => "Vicuna",
            ModelType::Llama(LlamaType::Wizardlm) => "Wizardlm",
            ModelType::GptNeoX(GptNeoXType::TinyPythia) => "Tiny Pythia",
            ModelType::GptNeoX(GptNeoXType::LargePythia) => "Large Pythia",
            ModelType::GptNeoX(GptNeoXType::Stablelm) => "Stablelm",
            ModelType::GptNeoX(GptNeoXType::DollySevenB) => "Dolly",
            ModelType::Mpt(MptType::Base) => "Mpt base",
            ModelType::Mpt(MptType::Chat) => "Mpt chat",
            ModelType::Mpt(MptType::Story) => "Mpt story",
            ModelType::Mpt(MptType::Instruct) => "Mpt instruct",
        }
    }
}

fn save_to_file<D: Serialize>(data: D) {
    let mut current_dir = std::env::current_dir().unwrap();
    current_dir.push("save.bin");
    match File::create(current_dir) {
        Ok(mut file) => {
            log::info!("serializing");
            match bincode::serialize(&data) {
                Ok(bytes) => {
                    log::info!("done serializing");
                    let result = file.write_all(&bytes);
                    log::info!("done writing {result:?}");
                }
                Err(err) => {
                    log::error!("{}", err)
                }
            }
        }
        Err(err) => {
            log::error!("{}", err)
        }
    }
}

fn get_from_file<D: DeserializeOwned>(create: impl FnOnce() -> D) -> D {
    let mut current_dir = std::env::current_dir().unwrap();
    current_dir.push("save.bin");
    if let Ok(mut file) = File::open(current_dir) {
        let mut buffer = Vec::new();

        if file.read_to_end(&mut buffer).is_err() {
            return create();
        }

        if let Ok(from_storage) = bincode::deserialize(&buffer[..]) {
            from_storage
        } else {
            create()
        }
    } else {
        create()
    }
}

#[tokio::main]
async fn main() {
    simple_logger::SimpleLogger::new()
        .with_level(LevelFilter::Off)
        .with_module_level("floneum", LevelFilter::Info)
        .init()
        .unwrap();

    let default_app = NodeGraphExample::default().await;

    eframe::run_native(
        "Floneum",
        eframe::NativeOptions::default(),
        Box::new(|cc| {
            cc.egui_ctx.set_pixels_per_point(1.0);
            cc.egui_ctx.set_visuals(Visuals::dark());
            let app: NodeGraphExample = get_from_file(|| default_app);
            Box::new(app)
        }),
    )
    .expect("Failed to run native example")
}

struct SetOutputMessage {
    node_id: NodeId,
    values: Vec<Value>,
}

#[derive(serde::Serialize, serde::Deserialize)]
pub struct MyNodeData {
    instance: PluginInstance,
    #[serde(skip)]
    run_count: usize,
    #[serde(skip)]
    running: bool,
    #[serde(skip)]
    queued: bool,
}

#[derive(Eq, serde::Serialize, serde::Deserialize, Debug)]
pub enum MyDataType {
    Single(MyPrimitiveDataType),
    List(MyPrimitiveDataType),
}

impl PartialEq for MyDataType {
    fn eq(&self, other: &Self) -> bool {
        let inner = match self {
            MyDataType::Single(inner) => inner,
            MyDataType::List(inner) => inner,
        };
        let other_inner = match other {
            MyDataType::Single(inner) => inner,
            MyDataType::List(inner) => inner,
        };
        inner == other_inner
    }
}

impl From<ValueType> for MyDataType {
    fn from(value: ValueType) -> Self {
        match value {
            ValueType::Single(value) => match value {
                PrimitiveValueType::Number => Self::Single(MyPrimitiveDataType::Number),
                PrimitiveValueType::Text => Self::Single(MyPrimitiveDataType::Text),
                PrimitiveValueType::Embedding => Self::Single(MyPrimitiveDataType::Embedding),
                PrimitiveValueType::Database => Self::Single(MyPrimitiveDataType::Database),
                PrimitiveValueType::Model => Self::Single(MyPrimitiveDataType::Model),
                PrimitiveValueType::ModelType => Self::Single(MyPrimitiveDataType::ModelType),
            },
            ValueType::Many(value) => match value {
                PrimitiveValueType::Number => Self::List(MyPrimitiveDataType::Number),
                PrimitiveValueType::Text => Self::List(MyPrimitiveDataType::Text),
                PrimitiveValueType::Embedding => Self::List(MyPrimitiveDataType::Embedding),
                PrimitiveValueType::Database => Self::List(MyPrimitiveDataType::Database),
                PrimitiveValueType::Model => Self::List(MyPrimitiveDataType::Model),
                PrimitiveValueType::ModelType => Self::List(MyPrimitiveDataType::ModelType),
            },
        }
    }
}

#[derive(PartialEq, Eq, serde::Serialize, serde::Deserialize, Debug, Clone, Copy)]
pub enum MyPrimitiveDataType {
    Number,
    Text,
    Embedding,
    Model,
    ModelType,
    Database,
}

#[derive(Clone, Debug, serde::Serialize, serde::Deserialize)]
pub enum MyValueType {
    Single(MyPrimitiveValueType),
    List(Vec<MyPrimitiveValueType>),
    Unset,
}

impl MyValueType {
    fn default_of_type(ty: &MyDataType) -> Self {
        match ty {
            MyDataType::Single(value) => Self::Single(MyPrimitiveValueType::default_of_type(*value)),
            MyDataType::List(value) => Self::List(vec![MyPrimitiveValueType::default_of_type(
                *value,
            )]),
        }
    }
}

#[derive(Serialize, Deserialize)]
#[serde(remote = "ModelType")]
enum ModelTypeDef {
    #[serde(with = "MptTypeDef")]
    Mpt(MptType),
    #[serde(with = "GptNeoXTypeDef")]
    GptNeoX(GptNeoXType),
    #[serde(with = "LlamaTypeDef")]
    Llama(LlamaType),
}

#[derive(Serialize, Deserialize)]
#[serde(remote = "MptType")]
enum MptTypeDef {
    Base,
    Story,
    Instruct,
    Chat,
}

#[derive(Serialize, Deserialize)]
#[serde(remote = "GptNeoXType")]
enum GptNeoXTypeDef {
    LargePythia,
    TinyPythia,
    DollySevenB,
    Stablelm,
}

#[derive(Serialize, Deserialize)]
#[serde(remote = "LlamaType")]
enum LlamaTypeDef {
    Vicuna,
    Guanaco,
    Wizardlm,
    Orca,
}

#[derive(Clone, Debug, serde::Serialize, serde::Deserialize)]
pub enum MyPrimitiveValueType {
    Number(i64),
    Text(String),
    Embedding(Vec<f32>),
    Model(u32),
    Database(u32),
    ModelType(#[serde(with = "ModelTypeDef")] ModelType),
}

impl MyPrimitiveValueType {
    fn default_of_type(ty: MyPrimitiveDataType) -> Self {
        match ty {
            MyPrimitiveDataType::Number => Self::Number(0),
            MyPrimitiveDataType::Text => Self::Text(String::new()),
            MyPrimitiveDataType::Embedding => Self::Embedding(vec![0.0; 512]),
            MyPrimitiveDataType::Model => Self::Model(0),
            MyPrimitiveDataType::Database => Self::Database(0),
            MyPrimitiveDataType::ModelType => Self::ModelType(ModelType::Llama(LlamaType::Vicuna)),
        }
    }
}

impl From<MyValueType> for Value {
    fn from(value: MyValueType) -> Self {
        match value {
            MyValueType::Single(value) => Self::Single(match value {
                MyPrimitiveValueType::Number(text) => PrimitiveValue::Number(text),
                MyPrimitiveValueType::Text(text) => PrimitiveValue::Text(text),
                MyPrimitiveValueType::Embedding(embedding) => {
                    PrimitiveValue::Embedding(Embedding { vector: embedding })
                }
                MyPrimitiveValueType::Model(id) => PrimitiveValue::Model(ModelId { id }),
                MyPrimitiveValueType::Database(id) => {
                    PrimitiveValue::Database(EmbeddingDbId { id })
                }
                MyPrimitiveValueType::ModelType(model_type) => {
                    PrimitiveValue::ModelType(model_type)
                }
            }),
            MyValueType::List(values) => Self::Many(
                values
                    .into_iter()
                    .map(|value| match value {
                        MyPrimitiveValueType::Number(text) => PrimitiveValue::Number(text),
                        MyPrimitiveValueType::Text(text) => PrimitiveValue::Text(text),
                        MyPrimitiveValueType::Embedding(embedding) => {
                            PrimitiveValue::Embedding(Embedding { vector: embedding })
                        }
                        MyPrimitiveValueType::Model(id) => PrimitiveValue::Model(ModelId { id }),
                        MyPrimitiveValueType::Database(id) => {
                            PrimitiveValue::Database(EmbeddingDbId { id })
                        }
                        MyPrimitiveValueType::ModelType(model_type) => {
                            PrimitiveValue::ModelType(model_type)
                        }
                    })
                    .collect(),
            ),
            MyValueType::Unset => todo!(),
        }
    }
}

impl From<Value> for MyValueType {
    fn from(value: Value) -> Self {
        match value {
            Value::Single(value) => Self::Single(match value {
                PrimitiveValue::Number(text) => MyPrimitiveValueType::Number(text),
                PrimitiveValue::Text(text) => MyPrimitiveValueType::Text(text),
                PrimitiveValue::Embedding(embedding) => {
                    MyPrimitiveValueType::Embedding(embedding.vector)
                }
                PrimitiveValue::Model(id) => MyPrimitiveValueType::Model(id.id),
                PrimitiveValue::Database(id) => MyPrimitiveValueType::Database(id.id),
                PrimitiveValue::ModelType(model_type) => {
                    MyPrimitiveValueType::ModelType(model_type)
                }
            }),
            Value::Many(values) => Self::List(
                values
                    .into_iter()
                    .map(|value| match value {
                        PrimitiveValue::Number(text) => MyPrimitiveValueType::Number(text),
                        PrimitiveValue::Text(text) => MyPrimitiveValueType::Text(text),
                        PrimitiveValue::Embedding(embedding) => {
                            MyPrimitiveValueType::Embedding(embedding.vector)
                        }
                        PrimitiveValue::Model(id) => MyPrimitiveValueType::Model(id.id),
                        PrimitiveValue::Database(id) => MyPrimitiveValueType::Database(id.id),
                        PrimitiveValue::ModelType(model_type) => {
                            MyPrimitiveValueType::ModelType(model_type)
                        }
                    })
                    .collect(),
            ),
        }
    }
}

impl Default for MyValueType {
    fn default() -> Self {
        Self::Unset
    }
}

#[derive(Clone, Copy, Debug, Deserialize, Serialize, PartialEq, Eq, Hash)]
pub struct PluginId(usize);

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum MyResponse {
    RunNode(NodeId),
    ClearNode(NodeId),
}

#[derive(Default, serde::Serialize, serde::Deserialize)]
pub struct MyGraphState {
    #[serde(skip)]
    pub plugin_engine: PluginEngine,
    pub plugins: slab::Slab<Plugin>,
    pub all_plugins: HashSet<PluginId>,
    #[serde(skip)]
    pub node_outputs: HashMap<OutputId, MyValueType>,
}

impl MyGraphState {
    fn get_plugin(&self, id: PluginId) -> &Plugin {
        &self.plugins[id.0]
    }
}

impl DataTypeTrait<MyGraphState> for MyDataType {
    fn data_type_color(&self, _user_state: &mut MyGraphState) -> egui::Color32 {
        match self {
            MyDataType::Single(MyPrimitiveDataType::Text) => egui::Color32::from_rgb(38, 109, 211),
            MyDataType::Single(MyPrimitiveDataType::Embedding) => {
                egui::Color32::from_rgb(238, 207, 109)
            }
            MyDataType::Single(MyPrimitiveDataType::Number) => egui::Color32::from_rgb(211, 38, 38),
            MyDataType::Single(MyPrimitiveDataType::Model) => egui::Color32::from_rgb(38, 211, 109),
            MyDataType::Single(MyPrimitiveDataType::Database) => {
                egui::Color32::from_rgb(38, 211, 109)
            }
            MyDataType::Single(MyPrimitiveDataType::ModelType) => {
                egui::Color32::from_rgb(38, 50, 109)
            }
            MyDataType::List(MyPrimitiveDataType::Text) => egui::Color32::from_rgb(38, 109, 211),
            MyDataType::List(MyPrimitiveDataType::Embedding) => {
                egui::Color32::from_rgb(238, 207, 109)
            }
            MyDataType::List(MyPrimitiveDataType::Number) => egui::Color32::from_rgb(211, 38, 38),
            MyDataType::List(MyPrimitiveDataType::Model) => egui::Color32::from_rgb(38, 211, 109),
            MyDataType::List(MyPrimitiveDataType::Database) => {
                egui::Color32::from_rgb(38, 211, 109)
            }
            MyDataType::List(MyPrimitiveDataType::ModelType) => {
                egui::Color32::from_rgb(38, 50, 109)
            }
        }
    }

    fn name(&self) -> Cow<'_, str> {
        match self {
            MyDataType::Single(MyPrimitiveDataType::Text) => Cow::Borrowed("text"),
            MyDataType::Single(MyPrimitiveDataType::Embedding) => Cow::Borrowed("embedding"),
            MyDataType::Single(MyPrimitiveDataType::Number) => Cow::Borrowed("number"),
            MyDataType::Single(MyPrimitiveDataType::Model) => Cow::Borrowed("model"),
            MyDataType::Single(MyPrimitiveDataType::Database) => Cow::Borrowed("database"),
            MyDataType::Single(MyPrimitiveDataType::ModelType) => Cow::Borrowed("model type"),
            MyDataType::List(MyPrimitiveDataType::Text) => Cow::Borrowed("list of texts"),
            MyDataType::List(MyPrimitiveDataType::Embedding) => Cow::Borrowed("list of embeddings"),
            MyDataType::List(MyPrimitiveDataType::Number) => Cow::Borrowed("list of numbers"),
            MyDataType::List(MyPrimitiveDataType::Model) => Cow::Borrowed("list of models"),
            MyDataType::List(MyPrimitiveDataType::Database) => Cow::Borrowed("list of databases"),
            MyDataType::List(MyPrimitiveDataType::ModelType) => {
                Cow::Borrowed("list of model types")
            }
        }
    }
}

impl NodeTemplateTrait for PluginId {
    type NodeData = MyNodeData;
    type DataType = MyDataType;
    type ValueType = MyValueType;
    type UserState = MyGraphState;
    type CategoryType = &'static str;

    fn node_finder_label(&self, user_state: &mut Self::UserState) -> Cow<'_, str> {
        Cow::Owned(user_state.get_plugin(*self).name())
    }

    // this is what allows the library to show collapsible lists in the node finder.
    fn node_finder_categories(&self, _user_state: &mut Self::UserState) -> Vec<&'static str> {
        vec!["Plugins"]
    }

    fn node_graph_label(&self, user_state: &mut Self::UserState) -> String {
        // It's okay to delegate this to node_finder_label if you don't want to
        // show different names in the node finder and the node itself.
        self.node_finder_label(user_state).into()
    }

    fn user_data(&self, user_state: &mut Self::UserState) -> Self::NodeData {
        MyNodeData {
            running: false,
            queued: false,
            run_count: 0,
            instance: user_state.get_plugin(*self).instance().block_on(),
        }
    }

    fn build_node(
        &self,
        graph: &mut Graph<Self::NodeData, Self::DataType, Self::ValueType>,
        _user_state: &mut Self::UserState,
        node_id: NodeId,
    ) {
        // The nodes are created empty by default. This function needs to take
        // care of creating the desired inputs and outputs based on the template

        let node = &graph[node_id];

        let meta = node.user_data.instance.metadata().clone();

        for input in &meta.inputs {
            let name = &input.name;
            let ty = input.ty.into();
            let value = MyValueType::default_of_type(&ty);
            match &ty {
                MyDataType::List(_) => {
                    graph.add_wide_input_param(
                        node_id,
                        name.to_string(),
                        ty,
                        value,
                        InputParamKind::ConnectionOrConstant,
                        None,
                        true,
                    );
                }
                MyDataType::Single(_) => {
                    graph.add_input_param(
                        node_id,
                        name.to_string(),
                        ty,
                        value,
                        InputParamKind::ConnectionOrConstant,
                        true,
                    );
                }
            }
        }

        for output in &meta.outputs {
            let name = &output.name;
            let ty: MyDataType = output.ty.into();
            graph.add_output_param(node_id, name.to_string(), ty);
        }
    }
}

pub struct AllMyNodeTemplates(Vec<PluginId>);

impl NodeTemplateIter for AllMyNodeTemplates {
    type Item = PluginId;

    fn all_kinds(&self) -> Vec<Self::Item> {
        // This function must return a list of node kinds, which the node finder
        // will use to display it to the user. Crates like strum can reduce the
        // boilerplate in enumerating all variants of an enum.
        self.0.clone()
    }
}

impl WidgetValueTrait for MyValueType {
    type Response = MyResponse;
    type UserState = MyGraphState;
    type NodeData = MyNodeData;
    fn value_widget(
        &mut self,
        param_name: &str,
        node_id: NodeId,
        ui: &mut egui::Ui,
        _user_state: &mut MyGraphState,
        _node_data: &MyNodeData,
    ) -> Vec<MyResponse> {
        // This trait is used to tell the library which UI to display for the
        // inline parameter widgets.
        egui::ScrollArea::vertical()
            .id_source((node_id, param_name))
            .show(ui, |ui| match self {
                MyValueType::Single(value) => {
                    ui.label(param_name);
                    match value {
                        MyPrimitiveValueType::Text(value) => {
                            ui.add(TextEdit::multiline(value));
                        }
                        MyPrimitiveValueType::Embedding(_) => {
                            ui.label("Embedding");
                        }
                        MyPrimitiveValueType::Model(_) => {
                            ui.label("Model");
                        }
                        MyPrimitiveValueType::Database(_) => {
                            ui.label("Database");
                        }
                        MyPrimitiveValueType::Number(value) => {
                            ui.add(DragValue::new(value));
                        }
                        MyPrimitiveValueType::ModelType(ty) => {
                            let name = ty.name();
                            ui.collapsing(name, |ui| {
                                ui.vertical(|ui| {
                                    for varient in ModelType::VARIANTS {
                                        if ui.button(varient.name()).clicked() {
                                            *ty = *varient;
                                        }
                                    }
                                })
                            });
                        }
                    }
                }
                MyValueType::List(values) => {
                    ui.label(param_name);
                    for value in values {
                        match value {
                            MyPrimitiveValueType::Text(value) => {
                                ui.add(TextEdit::multiline(value));
                            }
                            MyPrimitiveValueType::Embedding(_) => {
                                ui.label("Embedding");
                            }
                            MyPrimitiveValueType::Model(_) => {
                                ui.label("Model");
                            }
                            MyPrimitiveValueType::Database(_) => {
                                ui.label("Database");
                            }
                            MyPrimitiveValueType::Number(value) => {
                                ui.add(DragValue::new(value));
                            }
                            MyPrimitiveValueType::ModelType(ty) => {
                                let name = ty.name();
                                ui.collapsing(name, |ui| {
                                    ui.vertical(|ui| {
                                        for varient in ModelType::VARIANTS {
                                            if ui.button(varient.name()).clicked() {
                                                *ty = *varient;
                                            }
                                        }
                                    })
                                });
                            }
                        }
                    }
                }
                MyValueType::Unset => {}
            });

        Vec::new()
    }
}

impl UserResponseTrait for MyResponse {}
impl NodeDataTrait for MyNodeData {
    type Response = MyResponse;
    type UserState = MyGraphState;
    type DataType = MyDataType;
    type ValueType = MyValueType;

    fn bottom_ui(
        &self,
        ui: &mut egui::Ui,
        node_id: NodeId,
        graph: &Graph<MyNodeData, MyDataType, MyValueType>,
        user_state: &mut Self::UserState,
    ) -> Vec<NodeResponse<MyResponse, MyNodeData>>
    where
        MyResponse: UserResponseTrait,
    {
        let node = &graph[node_id];

        ui.label(format!("run count {}", self.run_count));

        if node.user_data.running {
            ui.with_layout(
                egui::Layout::centered_and_justified(egui::Direction::LeftToRight),
                |ui| {
                    ui.add(egui::widgets::Spinner::new());
                },
            );
            return vec![];
        }

        let run_button = ui.button("Run");
        if run_button.clicked() {
            return vec![NodeResponse::User(MyResponse::RunNode(node_id))];
        }
        let run_button = ui.button("Clear");
        if run_button.clicked() {
            return vec![NodeResponse::User(MyResponse::ClearNode(node_id))];
        }

        // Render the current output of the node
        let outputs = &node.outputs;

        for (name, id) in outputs {
            let value = user_state.node_outputs.get(id).cloned().unwrap_or_default();
            egui::ScrollArea::vertical()
                .id_source((node_id, name))
                .show(ui, |ui| match &value {
                    MyValueType::Single(single) => match single {
                        MyPrimitiveValueType::Text(value) => {
                            ui.label(value);
                        }
                        MyPrimitiveValueType::Embedding(value) => {
                            ui.label(format!("{:?}", &value[..5]));
                        }
                        MyPrimitiveValueType::Model(id) => {
                            ui.label(format!("Model: {id:?}"));
                        }
                        MyPrimitiveValueType::Database(id) => {
                            ui.label(format!("Database: {id:?}"));
                        }
                        MyPrimitiveValueType::Number(value) => {
                            ui.label(format!("{:02}", value));
                        }
                        MyPrimitiveValueType::ModelType(ty) => {
                            ui.label(ty.name());
                        }
                    },
                    MyValueType::List(many) => {
                        for value in many {
                            match value {
                                MyPrimitiveValueType::Text(value) => {
                                    ui.label(value);
                                }
                                MyPrimitiveValueType::Embedding(value) => {
                                    ui.label(format!("{:?}", &value[..5]));
                                }
                                MyPrimitiveValueType::Model(id) => {
                                    ui.label(format!("Model: {id:?}"));
                                }
                                MyPrimitiveValueType::Database(id) => {
                                    ui.label(format!("Database: {id:?}"));
                                }
                                MyPrimitiveValueType::Number(value) => {
                                    ui.label(format!("{:02}", value));
                                }
                                MyPrimitiveValueType::ModelType(ty) => {
                                    ui.label(ty.name());
                                }
                            }
                        }
                    }
                    MyValueType::Unset => {}
                });
        }

        vec![]
    }
}

type MyEditorState = GraphEditorState<MyNodeData, MyDataType, MyValueType, PluginId, MyGraphState>;

static PACKAGE_MANAGER: Lazy<Index> = Lazy::new(|| Index::new().unwrap());

#[derive(Serialize, Deserialize)]
pub struct NodeGraphExample {
    state: MyEditorState,

    user_state: MyGraphState,

    plugin_path_text: String,

    search_text: String,

    #[serde(skip)]
    txrx: TxRx,
}

impl NodeGraphExample {
    fn should_run_node(&self, id: NodeId) -> bool {
        // traverse back through inputs to see if any of those nodes are running
        let mut visited = HashSet::default();
        visited.insert(id);
        let mut should_visit = Vec::new();
        {
            // first add all of the inputs to the current node
            let node = &self.state.graph.nodes[id];
            if node.user_data.running {
                return false;
            }
            for (_, id) in &node.inputs {
                if let Some(node) = self.find_connected_node(*id) {
                    should_visit.push(node);
                    visited.insert(node);
                }
            }
        }

        while let Some(id) = should_visit.pop() {
            let node = &self.state.graph.nodes[id];
            if node.user_data.running || node.user_data.queued {
                return false;
            }
            for (_, id) in &node.inputs {
                if let Some(node) = self.find_connected_node(*id) {
                    if !visited.contains(&node) {
                        should_visit.push(node);
                        visited.insert(node);
                    }
                }
            }
        }

        true
    }

    fn find_connected_node(&self, input_id: InputId) -> Option<NodeId> {
        for (input, output) in self.state.graph.iter_connections() {
            if input == input_id {
                let node_id: NodeId = self.state.graph[output].node;
                return Some(node_id);
            }
        }
        None
    }

    fn run_node(&mut self, id: NodeId) {
        if !self.should_run_node(id) {
            println!(
                "node {:?} has unresolved dependancies, skipping running",
                id
            );
            return;
        }
        let node = &self.state.graph[id];

        let mut values: Vec<Value> = Vec::new();
        for (_, id) in &node.inputs {
            let input = self.state.graph.get_input(*id);
            let connection = self.state.graph.connections.get(input.id);
            let value = match connection {
                Some(connections) => {
                    let mut values = Vec::new();
                    for connection in connections {
                        let connection = self.state.graph.get_output(*connection);
                        let output_id = connection.id;
                        if let Some(value) = self.user_state.node_outputs.get(&output_id) {
                            match value {
                                MyValueType::List(items) => {
                                    for value in items {
                                        values.push(value.clone().into());
                                    }
                                }
                                MyValueType::Single(value) => {
                                    values.push(value.clone().into());
                                }
                                _ => return,
                            }
                        } else {
                            return;
                        }
                    }

                    match input.typ {
                        MyDataType::Single(_) => {
                            if values.len() != 1 {
                                return;
                            }
                            MyValueType::Single(values.pop().unwrap())
                        }
                        MyDataType::List(_) => {
                            values.reverse();
                            MyValueType::List(values)
                        },
                    }
                }
                None => input.value.clone().into(),
            };
            match &value {
                MyValueType::Unset => return,
                _ => values.push(value.into()),
            }
        }

        let fut = node.user_data.instance.run(values);
        let sender = self.txrx.tx.clone();
        self.state.graph[id].user_data.running = true;
        self.state.graph[id].user_data.run_count += 1;

        tokio::spawn(async move {
            let outputs = fut.await;

            let _ = sender
                .send(SetOutputMessage {
                    node_id: id,
                    values: outputs,
                })
                .await;
        });
    }

    fn clear_node(&mut self, id: NodeId) {
        self.state.graph[id].user_data.running = false;
        self.state.graph[id].user_data.queued = false;
        for (_, id) in &self.state.graph[id].outputs {
            self.user_state.node_outputs.remove(id);
        }
    }
}

impl Debug for NodeGraphExample {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("NodeGraphExample")
            .field("search_text", &self.search_text)
            .finish()
    }
}

impl NodeGraphExample {
    async fn default() -> Self {
        let mut user_state = MyGraphState::default();

        for package in PACKAGE_MANAGER.entries() {
            let path = package.path();
            let plugin = user_state.plugin_engine.load_plugin(&path).await;
            let id = user_state.plugins.insert(plugin);
            user_state.all_plugins.insert(PluginId(id));
        }

        Self {
            state: MyEditorState::default(),
            user_state,
            search_text: String::new(),
            plugin_path_text: String::new(),
            txrx: Default::default(),
        }
    }
}

struct TxRx {
    tx: Sender<SetOutputMessage>,
    rx: Receiver<SetOutputMessage>,
}

impl Default for TxRx {
    fn default() -> Self {
        let (tx, rx) = tokio::sync::mpsc::channel(100);

        Self { tx, rx }
    }
}

const PERSISTENCE_KEY: &str = "egui_node_graph";

impl NodeGraphExample {
    /// If the persistence feature is enabled, Called once before the first frame.
    /// Load previous app state (if any).
    pub fn new(cc: &eframe::CreationContext<'_>) -> Self {
        let state = cc
            .storage
            .and_then(|storage| eframe::get_value(storage, PERSISTENCE_KEY))
            .unwrap_or_default();

        Self {
            state,
            user_state: MyGraphState::default(),
            search_text: String::new(),
            plugin_path_text: String::new(),
            txrx: TxRx::default(),
        }
    }
}

impl eframe::App for NodeGraphExample {
    /// If the persistence function is enabled,
    /// Called by the frame work to save state before shutdown.
    fn save(&mut self, _: &mut dyn eframe::Storage) {
        println!("Saving state");
        save_to_file(self);
    }
    /// Called each time the UI needs repainting, which may be many times per second.
    /// Put your widgets into a `SidePanel`, `TopPanel`, `CentralPanel`, `Window` or `Area`.
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        // Recieve any async messages about setting node outputs.
        while let Ok(msg) = self.txrx.rx.try_recv() {
            let node = &self.state.graph[msg.node_id].outputs;
            for ((_, id), value) in node.iter().zip(msg.values.into_iter()) {
                self.user_state.node_outputs.insert(*id, value.into());
            }
            // stop this node's loading indicator
            self.state.graph[msg.node_id].user_data.running = false;
            self.state.graph[msg.node_id].user_data.queued = false;
            // start all connecting nodes
            let mut nodes_to_start = Vec::new();
            for (_, id) in &self.state.graph[msg.node_id].outputs {
                for (input, output) in self.state.graph.iter_connections() {
                    if output == *id {
                        let node_id = self.state.graph[input].node;
                        nodes_to_start.push(node_id);
                    }
                }
            }
            for node in &nodes_to_start {
                self.state.graph[*node].user_data.queued = true;
            }
            for node in nodes_to_start {
                self.run_node(node);
            }
        }

        egui::TopBottomPanel::top("top").show(ctx, |ui| {
            egui::menu::bar(ui, |ui| {
                egui::widgets::global_dark_light_mode_switch(ui);

                let response = ui.add(egui::TextEdit::singleline(&mut self.plugin_path_text));
                let button = ui.button("Load Plugin at path");
                if button.clicked()
                    || (response.lost_focus() && ui.input(|i| i.key_pressed(egui::Key::Enter)))
                {
                    let path = PathBuf::from(&self.plugin_path_text);
                    if path.exists() {
                        let plugin = self.user_state.plugin_engine.load_plugin(&path).block_on();
                        let id = self.user_state.plugins.insert(plugin);
                        self.user_state.all_plugins.insert(PluginId(id));
                    }
                }
            });
        });

        let graph_response = egui::CentralPanel::default().show(ctx, |ui| {
            if ui.input(|i| i.pointer.primary_down()) && ctx.input(|i| i.key_down(egui::Key::Space))
            {
                let delta = ui.input(|i| i.pointer.delta());
                self.state.pan_zoom.pan += delta;
                self.state.ongoing_box_selection = None;
            }
            self.state.draw_graph_editor(
                ui,
                AllMyNodeTemplates(self.user_state.all_plugins.iter().copied().collect()),
                &mut self.user_state,
                Vec::default(),
            )
        });
        let zoom_delta = ctx.input(|i| i.zoom_delta());
        if zoom_delta != 1.0 {
            self.state
                .pan_zoom
                .adjust_zoom(zoom_delta, Vec2::ZERO, 0.0, 50.0);
            let mut pixels_per_point = ctx.pixels_per_point();
            pixels_per_point *= zoom_delta;
            pixels_per_point = pixels_per_point.clamp(0.1, 50.0);
            pixels_per_point = (pixels_per_point * 10.).round() / 10.;
            ctx.set_pixels_per_point(pixels_per_point);
            let node_ids: Vec<_> = self.state.graph.nodes.iter().map(|(id, _)| id).collect();
            for id in node_ids {
                let center = graph_response.response.rect.center().to_vec2();
                let old_pos = self.state.node_positions[id].clone().to_vec2() - center;
                self.state.node_positions[id] = ((old_pos * zoom_delta) + center).to_pos2();
            }
        }
        let graph_response = graph_response.inner;

        for responce in graph_response.node_responses {
            match responce {
                NodeResponse::User(MyResponse::RunNode(id)) => {
                    self.run_node(id);
                }
                NodeResponse::User(MyResponse::ClearNode(id)) => {
                    self.clear_node(id);
                }
                _ => {}
            }
        }
    }
}
