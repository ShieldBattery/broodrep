# @shieldbattery/broodrep

WebAssembly bindings for the [broodrep](../broodrep/) StarCraft replay parser library.

## Installation

```bash
pnpm install @shieldbattery/broodrep
```

## Usage

### Browser

```javascript
import init, { parseReplayMetadata } from '@shieldbattery/broodrep'

await init()

const fileInput = document.getElementById('replay-file')
fileInput.addEventListener('change', async event => {
  const file = event.target.files[0]
  if (!file) return

  try {
    // Metadata is returned as ordinary JS data; no WASM owner needs to be freed.
    const metadata = parseReplayMetadata(new Uint8Array(await file.arrayBuffer()))
    console.log('Game title:', metadata.header.title)
    console.log('Map name:', metadata.header.mapName)
    console.log('Players:', metadata.slots.filter(player => !player.isEmpty && !player.isObserver))
  } catch (error) {
    console.error('Failed to parse replay:', error)
  }
})
```

### Node.js

```javascript
import * as fs from 'node:fs'
import { parseReplayMetadata } from '@shieldbattery/broodrep'

const replayData = new Uint8Array(fs.readFileSync('example.rep'))

try {
  const { format, header, slots } = parseReplayMetadata(replayData)
  console.log('Game:', header.title)
  console.log('Map:', header.mapName)
  console.log('Format:', format)
  console.log('Engine:', header.engine)
  console.log('Players:', slots.filter(player => !player.isEmpty && !player.isObserver).length)
} catch (error) {
  console.error('Failed to parse replay:', error)
}
```

## Choosing the Right API

Use the narrowest API that produces the result you need. This avoids parsing sections you will not
use and, especially, avoids constructing thousands of short-lived JavaScript command objects.

| What you need | Preferred API | Where the bulk data lives |
| --- | --- | --- |
| Header and player slots | `parseReplayMetadata` | Returned as small JS objects |
| Raw APM or command counts | `getPlayerApm` / `getCommandSummary` | Commands are streamed through WASM |
| One filtered command set | `queryCommands` | Only matches become JS objects |
| Repeated queries or pages | `loadCommands` | Complete parsed list stays in WASM |
| Every command at once | `getCommands` | Complete parsed list becomes JS objects |
| A range from a raw section | `loadRawSection` | Complete section stays in WASM |

### Metadata only: `parseReplayMetadata`

Use `parseReplayMetadata` for replay lists, uploads, indexing, or anything that only needs the
format, header, and slots. It returns ordinary JavaScript data and releases its copy of the replay
bytes before returning.

```javascript
const { format, header, slots } = parseReplayMetadata(replayData)
```

### Compact command analysis: specialized methods

If you need a summary rather than command contents, keep the work in WASM. These methods scan the
command stream without exporting every command:

```javascript
const replay = parseReplay(replayData)
try {
  const apm = replay.getPlayerApm()
  const counts = replay.getCommandSummary()
} finally {
  replay.free()
}
```

Every command method on `Replay` scans the command section independently. That is ideal for a
one-off compact result. If several filtered queries and/or APM calculations need the same parsed
commands, use `loadCommands` once instead.

### One command subset: `queryCommands`

Use `queryCommands` when JavaScript needs command contents, but only for a subset. Only matching
commands are retained and converted into JavaScript objects:

```javascript
const replay = parseReplay(replayData)
try {
  const production = replay.queryCommands({
    playerIds: [player.networkId],
    includeCategories: ['production'],
    endFrame: 24 * 60 * 5, // First five minutes at the usual 24 replay frames/second
  })
} finally {
  replay.free()
}
```

Command `playerId` values and query `playerIds` are network IDs matching `Player.networkId`, not
slot IDs.

### Repeated queries or paging: `loadCommands`

Use `loadCommands` when you will query the same command stream several times or page through it.
Parsing happens once and the complete command list remains in WASM memory. `query` and `getRange`
only copy their returned commands into JavaScript.

```javascript
const replay = parseReplay(replayData)
try {
  const commands = replay.loadCommands()
  if (commands) {
    try {
      const openingOrders = commands.query({
        endFrame: 24 * 60 * 5,
        includeCategories: ['order'],
      })
      const firstPage = commands.getRange(0, Math.min(500, commands.length))
      const apm = commands.getPlayerApm()
    } finally {
      commands.free()
    }
  }
} finally {
  replay.free()
}
```

### Complete eager export: `getCommands`

Use `getCommands` only when JavaScript genuinely needs every command at once. It is the simplest
compatibility API, but it has the highest peak allocation and JS/WASM boundary cost because the
entire stream becomes JavaScript objects immediately.

```javascript
const commands = replay.getCommands()
```

All section and command reads on `Replay` are uncached. Retain a result or use an explicit owner
when it will be reused. Likewise, methods such as `players()` return new JavaScript arrays, so store
their result rather than calling them repeatedly in a hot path.

`Replay`, `ParsedCommands`, and `RawSection` own WASM memory and expose `free()`. Release them in a
`finally` block when practical instead of waiting for JavaScript garbage collection. Values such as
`ReplayMetadata`, `ReplayHeader`, `Player[]`, query results, and APM summaries are ordinary copied
JavaScript data.

### Limits for untrusted replays

Decompression limits are selected when the replay is opened. Command limits are selected whenever
commands are scanned or loaded:

```javascript
const replay = parseReplay(replayData, {
  maxDecompressedSize: 100 * 1024 * 1024,
  maxCompressionRatio: 500,
})

try {
  const commands = replay.queryCommands(
    { excludeCategories: ['network'] },
    {
      maxCommands: 250_000,
      maxOwnedDataBytes: 16 * 1024 * 1024,
    },
  )
} finally {
  replay.free()
}
```

The shown values are the defaults, so most callers should omit them. All five command entry points
(`getCommands`, `loadCommands`, `queryCommands`, `getCommandSummary`, and `getPlayerApm`) accept the
same optional `CommandParseConfig`. Applications may lower the limits for a stricter resource
budget or raise them when larger trusted replays are expected.

`maxOwnedDataBytes` covers dynamically allocated command payloads such as selection unit tags,
chat text, and raw `known`/`unknown` command data. It does not count the fixed command records or the
decompressed command section itself.

## API Reference

### `parseReplay(data: Uint8Array, options?: DecompressionConfig | null): Replay`

Parses a replay header and returns a lazy owner for reading other sections on demand. The owner
retains a copy of the replay bytes in WASM memory until `free()` is called.

For metadata alone, prefer
`parseReplayMetadata(data, options?): { format: ReplayFormat; header: ReplayHeader; slots: Player[] }`.

**Parameters:**

- `data`: A `Uint8Array` containing the replay file bytes
- `options`: Optional decompression configuration to customize security limits

**Returns:** A `Replay` object with the following interface:

```typescript
class Replay {
  readonly format: ReplayFormat // "legacy", "modern", or "modern121"
  readonly header: ReplayHeader // Game header information

  // Methods for retrieving player information
  players(): Player[] // Active non-observer players
  observers(): Player[] // Active observers
  slots(): Player[] // All 12 slots, including empty slots
  hostPlayer(): Player | undefined // The host player if identifiable

  // Methods for retrieving raw section data
  getRawSection(section: ReplaySection): Uint8Array | undefined
  getRawCustomSection(section_id: number): Uint8Array | undefined
  loadRawSection(section: ReplaySection): RawSection | undefined
  loadRawCustomSection(section_id: number): RawSection | undefined

  // Command APIs
  getCommands(options?: CommandParseConfig | null): ReplayCommand[] | undefined
  loadCommands(options?: CommandParseConfig | null): ParsedCommands | undefined
  getCommandSummary(options?: CommandParseConfig | null): CommandSummary | undefined
  queryCommands(
    query: CommandQuery,
    options?: CommandParseConfig | null,
  ): ReplayCommand[] | undefined
  getPlayerApm(options?: CommandParseConfig | null): PlayerApmSummary | undefined

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
  race: Race // "z", "t", "p", or "r"
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
    Race,
    Race,
    Race,
    Race,
    Race,
    Race,
    Race,
    Race,
    Race,
    Race,
    Race,
    Race,
  ] // Starting race for each player
  gameId: string // Game UUID on ShieldBattery
  userIds: [number, number, number, number, number, number, number, number] // ShieldBattery user IDs
  gameLogicVersion: number | undefined // Game logic version (if available)
}
```

`loadRawSection` is the raw-section equivalent of `loadCommands`: it keeps the decompressed bytes in
WASM and `copyRange` transfers only the requested range. Use `getRawSection` when the complete
section is needed in JavaScript once.

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
  maxDecompressedSize: 200 * 1024 * 1024, // 200 MiB
  maxCompressionRatio: 1000.0, // Allow 1000:1 compression ratio
}

const replay = parseReplay(replayData, options)
try {
  // Read the sections needed by the application.
} finally {
  replay.free()
}
```

**Properties:**

- `maxDecompressedSize?: number` - Maximum bytes to decompress (default: 100 MiB). Prevents excessive memory usage.
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

const replay = parseReplay(replayData)
try {
  const shieldBatteryData = replay.getShieldBatterySection()

  if (shieldBatteryData) {
    console.log('Game ID:', shieldBatteryData.gameId)
    console.log('StarCraft Build:', shieldBatteryData.starcraftExeBuild)
    console.log('ShieldBattery Version:', shieldBatteryData.shieldbatteryVersion)

    if (shieldBatteryData.gameLogicVersion !== undefined) {
      console.log('Game Logic Version:', shieldBatteryData.gameLogicVersion)
    }

    const activeUserIds = shieldBatteryData.userIds.filter(id => id !== 0)
    console.log('User IDs:', activeUserIds)
    console.log('Starting Races:', shieldBatteryData.startingRaces)
  } else {
    console.log('No ShieldBattery data (normal for non-ShieldBattery replays)')
  }
} finally {
  replay.free()
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

The parse functions and lazy `Replay` methods throw JavaScript exceptions when parsing, limits, or
range validation fail. Catch failures at the application boundary, and use `finally` to release any
WASM owners that were successfully created.
