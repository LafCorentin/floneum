use floneum_rust::*;

#[export_plugin]
/// creates embeddings for text
fn embedding(input: String) -> Embedding {
    let model = ModelInstance::new(ModelType::Llama(LlamaType::Vicuna));

    model.get_embedding(&input)
}