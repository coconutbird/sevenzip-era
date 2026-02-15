# sevenzip-era

A 7-Zip plugin for reading and writing Ensemble ERA archive files.

## Overview

This plugin implements ERA archive format support for 7-Zip, allowing you to:

- **Extract** files from ERA archives
- **Create** new ERA archives
- **Update** existing ERA archives (add, remove, or replace files)

ERA is the archive format used by Ensemble Studios games. The archives use TEA (Tiny Encryption Algorithm) for encryption.

## Building

```bash
cargo build --release
```

The output DLL will be located at `target/release/era.dll`.

## Installation

1. Build the plugin with `cargo build --release`
2. Copy `era.dll` to your 7-Zip installation's `Formats` directory (e.g., `C:\Program Files\7-Zip\Formats\`)
3. Restart 7-Zip

## Dependencies

- [sevenzip-plugin](https://github.com/coconutbird/sevenzip-plugin) - Rust framework for creating 7-Zip plugins
- [era](https://github.com/coconutbird/ensemble-rs) - ERA archive format parser from ensemble-rs

## License

MIT
