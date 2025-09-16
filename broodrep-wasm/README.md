# @shieldbattery/broodrep

WebAssembly bindings for the [broodrep](../broodrep/) StarCraft replay parser library.

## Installation

```bash
pnpm install @shieldbattery/broodrep
```

## Usage

### Browser

```javascript
import init, { parseReplay, version } from '@shieldbattery/broodrep'

// Initialize the WASM module
await init()

// Load a replay file
const fileInput = document.getElementById('replay-file')
fileInput.addEventListener('change', async event => {
  const file = event.target.files[0]
  const arrayBuffer = await file.arrayBuffer()
  const uint8Array = new Uint8Array(arrayBuffer)

  try {
    const replayInfo = parseReplay(uint8Array)
    console.log('Replay parsed:', replayInfo)
    console.log('Game title:', replayInfo.gameTitle)
    console.log('Map name:', replayInfo.mapName)
    console.log('Players:', replayInfo.activePlayers)
  } catch (error) {
    console.error('Failed to parse replay:', error)
  }
})
```

### Node.js

```javascript
import * as fs from 'node:fs'
import { parseReplay, version } from '@shieldbattery/broodrep'

// Read replay file
const replayData = fs.readFileSync('example.rep')
const uint8Array = new Uint8Array(replayData)

try {
  const replayInfo = parseReplay(uint8Array)
  console.log('Game:', replayInfo.gameTitle)
  console.log('Map:', replayInfo.mapName)
  console.log('Format:', replayInfo.format)
  console.log('Engine:', replayInfo.engine)
  console.log('Players:', replayInfo.activePlayers.length)
} catch (error) {
  console.error('Failed to parse replay:', error)
}
```

## API Reference

### `parseReplay(data: Uint8Array, options?: DecompressionOptions): ReplayInfo`

Parses a StarCraft replay file and returns detailed information about the game.

**Parameters:**

- `data`: A `Uint8Array` containing the replay file bytes
- `options`: Optional decompression configuration to customize security limits

**Returns:** A `ReplayInfo` object containing:

```typescript
interface ReplayInfo {
  format: string // "Legacy (pre-1.18)", "Modern (1.18-1.21)", or "Modern (1.21+)"
  engine: string // "StarCraft" or "Brood War"
  frames: number // Number of game frames
  startTime: number | null // Unix timestamp of game start (or null if invalid)
  gameTitle: string // Game title
  mapName: string // Map name
  mapWidth: number // Map width in tiles
  mapHeight: number // Map height in tiles
  gameSpeed: string // Game speed setting
  gameType: string // Game type (e.g., "Melee", "Free For All")
  gameSubType: number // Game sub-type value
  hostName: string // Name of the game host
  players: PlayerInfo[] // All player slots (including empty)
  activePlayers: PlayerInfo[] // Only active players (non-empty, non-observers)
  observers: PlayerInfo[] // Only observers
}

interface PlayerInfo {
  slotId: number // Map slot ID
  networkId: number // Network ID
  playerType: string // "Human", "Computer", etc.
  race: string // "Terran", "Protoss", "Zerg", "Random"
  team: number // Team number
  name: string // Player name
  isEmpty: boolean // Whether this is an empty slot
  isObserver: boolean // Whether this is an observer
}
```

### `DecompressionOptions`

Configuration class for customizing security limits during replay parsing.

```javascript
import { DecompressionOptions } from './pkg/broodrep_wasm.js'

const options = new DecompressionOptions()
options.maxDecompressedSize = 200 * 1024 * 1024 // 200MB
options.maxCompressionRatio = 1000.0 // Allow 1000:1 compression ratio

const replayInfo = parseReplay(replayData, options)
```

**Properties:**

- `maxDecompressedSize?: number` - Maximum bytes to decompress (default: 100MB). Prevents excessive memory usage.
- `maxCompressionRatio?: number` - Maximum compression ratio allowed (default: 500:1). Higher ratios may indicate zip bomb attacks.

Note: Timing limits from the library are automatically disabled in WASM environments and cannot be
configured due to limitations of Rust's time implementation.

### `version(): string`

Returns the version of the broodrep library.

## Building

````bash
# Install wasm-pack if not already installed
cargo install wasm-pack

# Build
pnpm run build

## Testing

```bash
# Run WASM tests in nodejs
pnpm test
````

## Examples

See the [examples](./examples/) directory for complete usage examples:

- `index.html` - Interactive web demo with file upload
- `usage.js` - Comprehensive JavaScript examples

For the web version, run:

```bash
pnpm run dev
```

## Error Handling

The `parseReplay` function will throw if an error occurs. Always wrap calls in try-catch blocks for
proper error handling.
