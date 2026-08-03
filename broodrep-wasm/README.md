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
    console.log(
      'Players:',
      replay.players().filter(p => !p.isEmpty && !p.isObserver),
    )
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
  loadRawSection(section: ReplaySection): RawSection | undefined
  loadRawCustomSection(section_id: number): RawSection | undefined

  // Command APIs
  getCommands(options?: CommandParseConfig | null): ReplayCommand[] | undefined
  loadCommands(options?: CommandParseConfig): ParsedCommands | undefined
  getCommandSummary(options?: CommandParseConfig): CommandSummary | undefined
  queryCommands(
    query: CommandQuery,
    options?: CommandParseConfig,
  ): ReplayCommand[] | undefined
  getPlayerApm(options?: CommandParseConfig): PlayerApmSummary | undefined

  // Method for retrieving parsed ShieldBattery data
  getShieldBatterySection(): ShieldBatteryData | undefined
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

interface ShieldBatteryData {
  starcraftExeBuild: number // StarCraft executable build number
  shieldbatteryVersion: string // ShieldBattery client version
  teamGameMainPlayers: [number, number, number, number] // Main players in team games
  startingRaces: [
    number,
    number,
    number,
    number,
    number,
    number,
    number,
    number,
    number,
    number,
    number,
    number,
  ] // Starting race for each player
  gameId: string // Game UUID on ShieldBattery
  userIds: [number, number, number, number, number, number, number, number] // ShieldBattery user IDs
  gameLogicVersion: number | undefined // Game logic version (if available)
}
```

Section and command reads are uncached. Retain their result when it will be queried repeatedly.
`loadCommands` and `loadRawSection` make that ownership explicit while keeping the retained data in
WASM memory; their range methods copy only the requested page into JavaScript.

For metadata-only use cases, `parseReplayMetadata(data, options?)` returns the format, header, and
slots without retaining the complete replay byte buffer in WASM.

```typescript
interface CommandParseConfig {
  maxCommands?: number // Default: 250,000
  maxOwnedDataBytes?: number // Default: 16 MiB
}

class ParsedCommands {
  readonly length: number
  isEmpty(): boolean
  getRange(start: number, count: number): ReplayCommand[]
  query(query: CommandQuery): ReplayCommand[]
  getPlayerApm(): PlayerApmSummary
}

class RawSection {
  readonly length: number
  isEmpty(): boolean
  copyRange(start: number, count: number): Uint8Array
}

interface CommandSummary {
  total: number
  counts: Array<{ typeId: number; count: number }>
}

type CommandKind = ReplayCommand['command']['type']

type CommandCategory =
  | 'selection'
  | 'order'
  | 'production'
  | 'ability'
  | 'research'
  | 'hotkey'
  | 'diplomacy'
  | 'gameControl'
  | 'communication'
  | 'network'
  | 'playerStatus'
  | 'unknown'

interface CommandQuery {
  playerIds?: number[] // Player.networkId values, not slot IDs
  startFrame?: number // Inclusive
  endFrame?: number // Exclusive
  includeKinds?: CommandKind[]
  includeCategories?: CommandCategory[]
  includeTypeIds?: number[] // Raw wire-format IDs; useful for known/unknown commands
  excludeKinds?: CommandKind[]
  excludeCategories?: CommandCategory[]
  excludeTypeIds?: number[]
}

interface PlayerApmSummary {
  frames: number
  durationMinutes: number
  players: Array<{ playerId: number; actions: number; apm: number }>
}
```

Command queries preserve replay order. `playerIds` contains network IDs matching
`Player.networkId`, not slot indices. Player and frame constraints are intersected. If any
inclusion list is present, a command must match at least one included kind, category, or raw type
ID; the lists are unioned. Exclusions are applied afterward and always win. A present but empty
inclusion list intentionally matches no commands.

`getPlayerApm` calculates raw APM over the complete replay duration. It counts selections, orders,
production, abilities, research, hotkeys, diplomacy, and minimap pings. It excludes game-control
commands, chat, network traffic, player-status records, and untyped commands. It does not apply the
redundancy filtering used by effective-APM metrics.
The result uses `Player.networkId` values, includes only IDs with at least one counted action, and
may include an observer ID if that observer issued a counted command.

### `DecompressionConfig`

Configuration object for customizing security limits during replay parsing.

```javascript
// Create decompression config object
const options = {
  maxDecompressedSize: 200 * 1024 * 1024, // 200MB
  maxCompressionRatio: 1000.0, // Allow 1000:1 compression ratio
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

## ShieldBattery Support

The library includes support for parsing ShieldBattery-specific data from replays created through the [ShieldBattery](https://shieldbattery.net/) platform. This data provides additional context about games played on ShieldBattery.

### Basic Usage

```javascript
import { parseReplay } from '@shieldbattery/broodrep'

// Parse a replay
const replay = parseReplay(replayData)

// Check for ShieldBattery data
const shieldBatteryData = replay.getShieldBatterySection()

if (shieldBatteryData) {
  console.log('Game ID:', shieldBatteryData.gameId)
  console.log('StarCraft Build:', shieldBatteryData.starcraftExeBuild)
  console.log('ShieldBattery Version:', shieldBatteryData.shieldbatteryVersion)

  // Game logic version (if available in newer format)
  if (shieldBatteryData.gameLogicVersion !== undefined) {
    console.log('Game Logic Version:', shieldBatteryData.gameLogicVersion)
  }

  // User IDs of active players
  const activeUserIds = shieldBatteryData.userIds.filter(id => id !== 0)
  console.log('User IDs:', activeUserIds)

  // Starting races as numbers (0=Zerg, 1=Terran, 2=Protoss, 6=Random)
  const activePlayers = replay.players().filter(p => !p.isEmpty && !p.isObserver)
  const startingRaces = shieldBatteryData.startingRaces.slice(0, activePlayers.length)
  console.log('Starting Races:', startingRaces)
} else {
  console.log('No ShieldBattery data (normal for non-ShieldBattery replays)')
}
```

### ShieldBatteryData Fields

- **`gameId`**: Unique UUID for the game on ShieldBattery platform
- **`starcraftExeBuild`**: Build number of the StarCraft executable used
- **`shieldbatteryVersion`**: Version string of the ShieldBattery client
- **`gameLogicVersion`**: Version of game logic modifications (if available)
- **`userIds`**: Array of ShieldBattery user IDs corresponding to players
- **`teamGameMainPlayers`**: Identifies main players in team games
- **`startingRaces`**: Original race selection for each player slot (before randomization)

## Building

```bash
# Install wasm-pack if not already installed
cargo install wasm-pack

# Build
pnpm run build
```

The default feature set installs richer panic messages. Production builds that do not need the
panic hook can omit it (currently saving about 2.7 KiB from the optimized uncompressed module):

```bash
wasm-pack build --target web -- --no-default-features
```

## Testing

```bash
# Run WASM tests in nodejs
pnpm test
```

## Examples

See the [examples](./examples/) directory for complete usage examples:

- `index.html` - Interactive upload demo with player APM and filtered production commands
- `usage.mjs` - Node examples covering metadata, filtered queries, retained commands, and APM

Run the Node examples with `pnpm run example`.

For the web version, run:

```bash
pnpm run dev
```

## Error Handling

The `parseReplay` function will throw if an error occurs. Always wrap calls in try-catch blocks for
proper error handling.
