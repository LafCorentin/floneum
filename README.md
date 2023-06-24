# Floneum

Floneum is a graph editor for AI workflows with a focus on community made plugins, local AI and safety.

<img width="1512" alt="Screenshot 2023-06-18 at 4 26 11 PM" src="https://github.com/Demonthos/floneum/assets/66571940/c60d621d-72b9-423c-b1d5-57cdb737e449">

## Features

- Visual interface: You can use Floneum without any knowledge of programming. The visual graph editor makes it easy to combine community made plugins with local AI models
- Instantly run local large language models: Floneum does not require any external dependencies or even a GPU to run. It uses [llm](https://github.com/rustformers/llm) to run large language models locally. Because of this, you can run Floneum with your data without worrying about privacy.
- Plugins: By combining large language models with plugins, you can improve their performance and teach them about your proprietary data
- Create AI workflows: Combine plugins to create and share your workflow
- Safety with WASM plugins: All plugins run in an isolated environment. You don't need to trust any plugins you load. Plugins can only interact with their environment in a predefined safe API. They are isolated from the rest of your system to avoid access to the file system, and other potentially system APIs that can be used to harm your system.
- Multi-language plugins: Plugins can use in any language that supports web assembly. In addition to the API that can be accessed in any language, Floneum has a rust wrapper with ergonomic macros that make it simple to create plugins.
- Controlled inferance: Plugins can control the output of the large language models with a process similar to jsonformer or guidence. This allows plugins to force models to output valid json, or any other structure they define. This can be useful when communicating between a lauage model and a typed API

## Contributing

- Report issues on our [issue tracker](https://github.com/floneum/floneum/issues).
- Help other users in the Floneum discord
- If you are interested in contributing, reach out on discord