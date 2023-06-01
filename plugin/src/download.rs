use tokio::{fs::File, io::AsyncWriteExt, runtime::Handle};
use std::{path::PathBuf, error::Error,  time::Instant};
use futures_util::StreamExt;
use llm::{Model, LoadProgress, ModelArchitecture};
use spinoff::{Spinner, spinners::Dots2};

use crate::plugins::main::imports::{ModelType, LlamaType, GptNeoXType, MptType};


fn load_progress_callback(
    mut sp: Option<Spinner>,
    now: Instant,
    mut prev_load_time: Instant,
) -> impl FnMut(LoadProgress) {
    move |progress| match progress {
        LoadProgress::HyperparametersLoaded => {
            if let Some(sp) = sp.as_mut() {
                sp.update_text("Loaded hyperparameters")
            };
        }
        LoadProgress::ContextSize { bytes } => log::debug!(
            "ggml ctx size = {}",
            bytesize::to_string(bytes as u64, false)
        ),
        LoadProgress::TensorLoaded {
            current_tensor,
            tensor_count,
            ..
        } => {
            if prev_load_time.elapsed().as_millis() > 500 {
                // We don't want to re-render this on every message, as that causes the
                // spinner to constantly reset and not look like it's spinning (and
                // it's obviously wasteful).
                if let Some(sp) = sp.as_mut() {
                    sp.update_text(format!(
                        "Loaded tensor {}/{}",
                        current_tensor + 1,
                        tensor_count
                    ));
                };
                prev_load_time = std::time::Instant::now();
            }
        }
        LoadProgress::LoraApplied { name, source } => {
            if let Some(sp) = sp.as_mut() {
                sp.update_text(format!(
                    "Applied LoRA: {} from '{}'",
                    name,
                    source.file_name().unwrap().to_str().unwrap()
                ));
            };
        }
        LoadProgress::Loaded {
            file_size,
            tensor_count,
        } => {
            if let Some(sp) = sp.take() {
                sp.success(&format!(
                    "Loaded {tensor_count} tensors ({}) after {}ms",
                    bytesize::to_string(file_size, false),
                    now.elapsed().as_millis()
                ));
            };
        }
    }
}


pub fn download(model_type: ModelType) -> Box<dyn Model> {
    // https://www.reddit.com/r/LocalLLaMA/wiki/models/
    let url = match model_type {
        ModelType::Llama(LlamaType::Vicuna) => {
            "https://huggingface.co/CRD716/ggml-vicuna-1.1-quantized/resolve/main/ggml-vicuna-13B-1.1-q4_0.bin"
        }
        ModelType::Llama(LlamaType::Guanaco) => {
            "https://huggingface.co/TheBloke/guanaco-7B-GGML/resolve/main/guanaco-7B.ggmlv3.q4_0.bin"
        }
        ModelType::Llama(LlamaType::Wizardlm) => {
            "https://huggingface.co/TehVenom/WizardLM-13B-Uncensored-Q5_1-GGML/blob/main/WizardML-Unc-13b-Q5_1.bin"
        }
        ModelType::GptNeoX(GptNeoXType::Stablelm) => {
            "https://huggingface.co/cakewalk/ggml-q4_0-stablelm-tuned-alpha-7b/resolve/main/ggml-model-stablelm-tuned-alpha-7b-q4_0.bin"
        }
        ModelType::GptNeoX(GptNeoXType::DollySevenB) => {
            "https://huggingface.co/mverrilli/dolly-v2-7b-ggml/resolve/main/ggml-model-f16.bin"
        }
        ModelType::GptNeoX(GptNeoXType::TinyPythia) => {
            "https://huggingface.co/rustformers/pythia-ggml/resolve/main/pythia-70m-q4_0.bin"
        }
        ModelType::GptNeoX(GptNeoXType::LargePythia) => {
            "https://huggingface.co/rustformers/pythia-ggml/resolve/main/pythia-2.8b-q4_0.bin"
        }
        ModelType::Mpt(MptType::Chat) => {
            "https://huggingface.co/rustformers/mpt-7b-ggml/resolve/main/mpt-7b-chat-q4_0.bin"
        }
        ModelType::Mpt(MptType::Story) => {
            "https://huggingface.co/rustformers/mpt-7b-ggml/resolve/main/mpt-7b-storywriter-q4_0.bin"
        }
        ModelType::Mpt(MptType::Instruct) => {
            "https://huggingface.co/rustformers/mpt-7b-ggml/resolve/main/mpt-7b-instruct-q4_0.bin"
        }
        ModelType::Mpt(MptType::Base) => {
            "https://huggingface.co/rustformers/mpt-7b-ggml/resolve/main/mpt-7b-q4_0.bin"
        }
    };
    let archetecture = match model_type {
        ModelType::Llama(_) => ModelArchitecture::Llama,
        ModelType::GptNeoX(_) => ModelArchitecture::GptNeoX,
        ModelType::Mpt(_) => ModelArchitecture::Mpt,
    };
    let context_size = match model_type{
        ModelType::Llama(_) => 2024,
        ModelType::GptNeoX(GptNeoXType::Stablelm) => 4048,
        ModelType::GptNeoX(_)=>2048,
        ModelType::Mpt(MptType::Story) => 65_000,
        ModelType::Mpt(_) => 2024
    };

    let handle = Handle::current();
    let path = std::thread::spawn(move || {
        // Using Handle::block_on to run async code in the new thread.
        handle.block_on(async { download_model(url).await.unwrap() })
    })
    .join()
    .unwrap();

    let sp = Some(Spinner::new(Dots2, "Loading model...", None));

    let now = Instant::now();
    let prev_load_time = now;

    let model_params = llm::ModelParameters {
        prefer_mmap: true,
        context_size,
        lora_adapters: None,
    };

    llm::load_dynamic(
        archetecture,
        &path,
        llm::VocabularySource::Model,
        model_params,
        load_progress_callback(sp, now, prev_load_time),
    )
    .unwrap_or_else(|err| panic!("Failed to load model from {path:?}: {err}"))
}

async fn download_model(model_url: &str) -> Result<PathBuf, Box<dyn Error>> {
    let path: PathBuf = format!("./{}", model_url.rsplit_once('/').unwrap().1).into();
    if path.exists() {
        return Ok(path);
    }
    let response = reqwest::get(model_url).await?;
    println!("downloading model. This will take several minutes");

    let mut file = { File::create(&path).await? };

    let size = response.content_length().unwrap_or(4_294_967_296) as usize;

    let mut stream = response.bytes_stream();
    let mut current_size = 0;
    let mut old_precent = 0;
    while let Some(chunk_result) = stream.next().await {
        let chunk = chunk_result?;
        current_size += chunk.len();
        file.write_all(&chunk).await?;
        let new_precent = current_size * 100 / size;
        if old_precent != new_precent {
            println!("{}%", new_precent);
        }
        old_precent = new_precent;
    }

    file.flush().await?;

    println!("Finished Downloading");

    Ok(path)
}