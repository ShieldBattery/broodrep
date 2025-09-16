# broodrep

A pure Rust library for reading StarCraft 1 replay files. Supports all versions.

## Components

This project is organized as a Cargo workspace containing:

- **[broodrep](./broodrep/)** - The core library for parsing StarCraft replay files
- **[broodrep-cli](./broodrep-cli/)** - A command-line interface for the library
- **[broodrep-wasm](./broodrep-wasm/)** - WebAssembly bindings for browser and Node.js usage

## WebAssembly Support

Use broodrep in web browsers and Node.js with the WebAssembly bindings:

```javascript
import init, { parseReplay } from '@shieldbattery/broodrep';

await init(); // Initialize WASM module

const replayData = new Uint8Array(/* your replay file bytes */);
const replayInfo = parseReplay(replayData);

console.log('Game:', replayInfo.gameTitle);
console.log('Map:', replayInfo.mapName);
console.log('Players:', replayInfo.activePlayers);
```

See [broodrep-wasm](./broodrep-wasm/) for complete documentation and examples.

## Development

Test data is stored in Git LFS. If you haven't used Git LFS before, run:

```
git lfs install
```

## See also

- [broodmap](https://github.com/ShieldBattery/broodmap) - a pure Rust implementation of StarCraft 1 map parsing

## License

Licensed under either of

* Apache License, Version 2.0
  ([LICENSE-APACHE](LICENSE-APACHE) or http://www.apache.org/licenses/LICENSE-2.0)
* MIT license
  ([LICENSE-MIT](LICENSE-MIT) or http://opensource.org/licenses/MIT)

at your option.

## Contribution

Unless you explicitly state otherwise, any contribution intentionally submitted
for inclusion in the work by you, as defined in the Apache-2.0 license, shall be
dual licensed as above, without any additional terms or conditions.

