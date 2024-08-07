<h1 align="center">Floneum</h1>
<div align="center">
  <!-- Crates version -->
  <a href="https://crates.io/crates/kalosm">
    <img src="https://img.shields.io/crates/v/kalosm.svg?style=flat-square"
    alt="Crates.io version" />
  </a>
  <!-- Downloads -->
  <a href="https://crates.io/crates/kalosm">
    <img src="https://img.shields.io/crates/d/kalosm.svg?style=flat-square"
      alt="Download" />
  </a>
  <!-- docs -->
  <a href="https://docs.rs/kalosm">
    <img src="https://img.shields.io/badge/docs-latest-blue.svg?style=flat-square"
      alt="docs.rs docs" />
  </a>
  <!-- Discord -->
  <a href="https://discord.gg/dQdmhuB8q5">
    <img src="https://img.shields.io/discord/1120130300236800062?logo=discord&style=flat-square" alt="Discord Link" />
  </a>
</div>

Floneum makes it easy to develop applications that use local pre-trained AI models. There are two main projects in this monorepo:

- [Kalosm](./interfaces/kalosm): A simple interface for pre-trained models in rust
- [Floneum Editor (preview)](./floneum/floneum): A graphical editor for local AI workflows. See the [user documentation](https://floneum.com/docs/user/) or [plugin documentation](https://floneum.com/docs/developer/) for more information.

## Kalosm

[Kalosm](./interfaces/kalosm/) is a simple interface for pre-trained models in Rust that backs Floneum. It makes it easy to interact with pre-trained, language, audio, and image models.

### Model Support

Kalosm supports a variety of models. Here is a list of the models that are currently supported:

| Model    | Modality | Size | Description | Quantized | CUDA + Metal Accelerated | Example |
| -------- | ------- | ---- | ----------- | --------- | ----------- | --------------------- |
| Llama | Text    | 1b-70b | General purpose language model | ✅ | ✅ | [llama 3 chat](interfaces/kalosm/examples/chat.rs) |
| Mistral | Text    | 7-13b | General purpose language model | ✅ | ✅ | [mistral chat](interfaces/kalosm/examples/chat-mistral-2.rs) |
| Phi | Text    | 2b-4b | Small reasoning focused language model | ✅ | ✅ | [phi 3 chat](interfaces/kalosm/examples/chat-phi-3.rs) |
| Whisper | Audio   | 20MB-1GB | Audio transcription model | ✅ | ✅ | [live whisper transcription](interfaces/kalosm/examples/transcribe.rs) |
| RWuerstchen | Image | 5gb | Image generation model | ❌ | ✅ | [rwuerstchen image generation](interfaces/kalosm/examples/generate-image.rs) |
| TrOcr | Image | 3gb | Optical character recognition model | ❌ | ✅ | [Text Recognition](interfaces/kalosm/examples/ocr.rs) |
| Segment Anything | Image | 50MB-400MB | Image segmentation model | ❌ | ❌ | [Image Segmentation](interfaces/kalosm/examples/segment-image.rs) |
| Bert | Text    | 100MB-1GB | Text embedding model | ❌ | ✅ | [Semantic Search](interfaces/kalosm/examples/semantic-search.rs) |

### Utilities

Kalosm also supports a variety of utilities around pre-trained models. These include:
- [Extracting, formatting and retrieving context for LLMs](./interfaces/kalosm/examples/context_extraction.rs): [Extract context from txt/html/docx/md/pdf](./interfaces/kalosm/examples/context_extraction.rs) [chunk that context](./interfaces/kalosm/examples/chunking.rs) [then search for relevant context with vector database integrations](./interfaces/kalosm/examples/semantic-search.rs)
- [Transcribing audio from your microphone or file](./interfaces/kalosm/examples/transcribe.rs)
- [Crawling and scraping content from web pages](./interfaces/kalosm/examples/crawl.rs)

### Performance

Kalosm uses the [candle](https://github.com/huggingface/candle) machine learning library to run models in pure rust. It supports quantized and accelerated models with performance on par with `llama.cpp`:

**Mistral 7b** 
| Accelerator | Kalosm | llama.cpp |
| ------ | --------- | --------- |
| Metal (M2) | 26 t/s | 27 t/s |

### Structured Generation

Kalosm supports structured generation with arbitrary parsers. It uses a custom parser engine and sampler and structure-aware acceleration to make structure generation even faster than uncontrolled text generation:

```rust
use kalosm::language::*;

#[tokio::main]
async fn main() -> anyhow::Result<()> {
  // Download phi-3 or get the model from the cache
  let llm = Llama::phi_3().await?;

  // Derive an efficient parser for your struct with the `Parse` trait
  #[derive(Debug, Clone, Parse)]
  struct Pet {
    name: String,
    description: String,
    color: String,
    size: Size,
    diet: Diet,
  }

  #[derive(Debug, Clone, Parse)]
  enum Diet {
    Carnivore,
    Herbivore,
    Omnivore,
  }
      
  #[derive(Debug, Clone, Parse)]
  enum Size {
    Small,
    Medium,
    Large,
  }

  let task = Task::builder("You generate realistic JSON placeholders")
    .with_constraints(<[Pet; 4] as Parse>::new_parser())
    .build();
  let mut stream = task.run("Generate a list of 4 pets in JSON form with a name, description, color, and diet", &llm);

  stream.to_std_out().await?;

  Ok(())
}
```

[structured generation demo](https://github.com/floneum/floneum/assets/66571940/7bdee1fb-204f-4e24-b738-1d4da6b593d9)

In addition to regex, you can provide your own grammar to generate structured data. This lets you constrain the response to any structure you want including complex data structures like JSON, HTML, and XML.

### Kalosm Quickstart!

This quickstart will get you up and running with a simple chatbot. Let's get started!

> A more complete guide for Kalosm is available on the [Kalosm website](https://floneum.com/kalosm/), and examples are available in the [examples folder](https://github.com/floneum/floneum/tree/main/interfaces/kalosm/examples).

1) Install [rust](https://rustup.rs/)
2) Create a new project:
```sh
cargo new kalosm-hello-world
cd ./kalosm-hello-world
```
3) Add Kalosm as a dependency
```sh
# You can use `--features language,metal`, `--features language,cuda`, or `--features language,mkl` if your machine supports an accelerator
cargo add kalosm --git https://github.com/floneum/floneum --features language
cargo add tokio --features full
```
4) Add this code to your `main.rs` file
```rust, no_run
use kalosm::language::*;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
  let model = Llama::phi_3().await?;
  let mut chat = Chat::builder(model)
    .with_system_prompt("You are a pirate called Blackbeard")
    .build();

  loop {
    chat.add_message(prompt_input("\n> ")?)
      .to_std_out()
      .await?;
  }
}
```

5) Run your application with:

```sh
cargo run --release
```

[chat bot demo](https://github.com/floneum/floneum/assets/66571940/e4e76efb-6387-4fcd-aa3c-aa556e840334)

## Community

If you are interested in either project, you can join the [discord](https://discord.gg/dQdmhuB8q5) to discuss the project and get help.

## Contributing

- Report issues on our [issue tracker](https://github.com/floneum/floneum/issues).
- Help other users in the discord
- If you are interested in contributing, feel free to reach out on discord
