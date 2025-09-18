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
    const replay = parseReplay(uint8Array)
    const header = replay.header
    console.log('Replay parsed:', replay)
    console.log('Game title:', header.title)
    console.log('Map name:', header.mapName)
    console.log('Players:', replay.players().filter(p => !p.isEmpty && !p.isObserver))
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
  const replay = parseReplay(uint8Array)
  const header = replay.header
  console.log('Game:', header.title)
  console.log('Map:', header.mapName)
  console.log('Format:', replay.format)
  console.log('Engine:', header.engine)
  console.log('Players:', replay.players().filter(p => !p.isEmpty && !p.isObserver).length)
} catch (error) {
  console.error('Failed to parse replay:', error)
}
```

## API Reference

### `parseReplay(data: Uint8Array, options?: DecompressionConfig): Replay`

Parses a StarCraft replay file and returns a Replay object for retrieving game information.

**Parameters:**

- `data`: A `Uint8Array` containing the replay file bytes
- `options`: Optional decompression configuration to customize security limits

**Returns:** A `Replay` object with the following interface:

```typescript
class Replay {
  readonly format: ReplayFormat // "legacy", "modern", or "modern121"
  readonly header: ReplayHeader // Game header information
  
  // Methods for retrieving player information
  players(): Player[] // All player slots (including empty)
  observers(): Player[] // Only observers
  slots(): Player[] // All slots
  hostPlayer(): Player | undefined // The host player if identifiable
  
  // Methods for retrieving raw section data
  getRawSection(section: ReplaySection): Uint8Array | undefined
  getRawCustomSection(section_id: number): Uint8Array | undefined
}

interface ReplayHeader {
  engine: Engine // "starCraft", "broodWar", or "unknown"
  frames: number // Number of game frames
  startTime: number // Unix timestamp of game start
  title: string // Game title
  mapWidth: number // Map width in tiles
  mapHeight: number // Map height in tiles
  availableSlots: number // Number of available player slots
  speed: GameSpeed // Game speed setting
  gameType: GameType // Game type (e.g., "melee", "freeForAll")
  gameSubType: number // Game sub-type value
  hostName: string // Name of the game host
  mapName: string // Map name
}

interface Player {
  slotId: number // Map slot ID (post-randomization)
  networkId: number // Network ID (255 for computer, 128-131 for observers)
  playerType: PlayerType // "inactive", "computer", "human", etc.
  race: Race // "zerg", "terran", "protoss", "random"
  team: number // Team number
  name: string // Player name
  isEmpty: boolean // Whether this is an empty slot
  isObserver: boolean // Whether this is an observer
}
```

### `DecompressionConfig`

Configuration object for customizing security limits during replay parsing.

```javascript
// Create decompression config object
const options = {
  maxDecompressedSize: 200 * 1024 * 1024, // 200MB
  maxCompressionRatio: 1000.0 // Allow 1000:1 compression ratio
}

const replay = parseReplay(replayData, options)
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
- `usage.mjs` - Comprehensive JavaScript examples

For the web version, run:

```bash
pnpm run dev
```

## Error Handling

The `parseReplay` function will throw if an error occurs. Always wrap calls in try-catch blocks for
proper error handling.
