/**
 * broodrep-wasm Usage Examples
 *
 * Run with: node usage.js
 * (Make sure to build the WASM package first: npm run build:node)
 */

import fs from 'fs'
import path from 'path'

// Import the WASM module
// Note: In a real project, you would install from npm and import normally:
// import { parseReplay, version } from '@shieldbattery/broodrep';
import { init, parseReplay, version } from '../pkg-node/broodrep_wasm.js'

// Initialize the WASM module
init()

/**
 * Example 1: Basic replay parsing
 */
function basicExample() {
  console.log('=== Basic Example ===')
  console.log(`broodrep-wasm version: ${version()}`)

  const testReplayPath = path.join(
    import.meta.dirname,
    '..',
    '..',
    'broodrep',
    'testdata',
    'things.rep',
  )

  try {
    // Read the replay file
    const replayData = fs.readFileSync(testReplayPath)
    const uint8Array = new Uint8Array(replayData)

    // Parse the replay
    const replay = parseReplay(uint8Array)
    const header = replay.header

    console.log('✓ Successfully parsed replay!')
    console.log(`  Title: ${header.title}`)
    console.log(`  Map: ${header.mapName}`)
    console.log(`  Format: ${replay.format}`)
    console.log(`  Engine: ${header.engine}`)
    console.log(`  Players: ${replay.players().filter(p => !p.isEmpty && !p.isObserver).length}`)
    console.log(`  Observers: ${replay.observers().length}`)
  } catch (error) {
    console.error('✗ Failed to parse replay:', error)
  }
}

/**
 * Example 2: Detailed information extraction
 */
function detailedExample() {
  console.log('\n=== Detailed Example ===')

  const testReplayPath = path.join(
    import.meta.dirname,
    '..',
    '..',
    'broodrep',
    'testdata',
    'scr_replay.rep',
  )

  if (!fs.existsSync(testReplayPath)) {
    console.log('Test replay not found, skipping detailed example...')
    return
  }

  try {
    const replayData = fs.readFileSync(testReplayPath)
    const uint8Array = new Uint8Array(replayData)
    const replay = parseReplay(uint8Array)
    const header = replay.header

    // Game information
    console.log('Game Information:')
    console.log(`  Title: ${header.title}`)
    console.log(`  Format: ${replay.format}`)
    console.log(`  Engine: ${header.engine}`)
    console.log(`  Game Type: ${header.gameType}`)
    console.log(`  Speed: ${header.speed}`)
    console.log(`  Frames: ${header.frames.toLocaleString()}`)

    if (header.startTime) {
      const startDate = new Date(header.startTime * 1000)
      console.log(`  Started: ${startDate.toLocaleString()}`)
    }

    // Map information
    console.log('\nMap Information:')
    console.log(`  Name: ${header.mapName}`)
    console.log(`  Dimensions: ${header.mapWidth} × ${header.mapHeight}`)

    // Host information
    console.log('\nHost Information:')
    console.log(`  Host: ${header.hostName || 'Unknown'}`)

    // Players
    const activePlayers = replay.players().filter(p => !p.isEmpty && !p.isObserver)
    console.log('\nActive Players:')
    activePlayers.forEach((player, index) => {
      console.log(`  ${index + 1}. ${player.name}`)
      console.log(`     Race: ${player.race}`)
      console.log(`     Team: ${player.team}`)
      console.log(`     Type: ${player.playerType}`)
      console.log(`     Slot: ${player.slotId}`)
    })

    // Observers
    const observers = replay.observers()
    if (observers.length > 0) {
      console.log('\nObservers:')
      observers.forEach((observer, index) => {
        console.log(`  ${index + 1}. ${observer.name} (${observer.playerType})`)
      })
    }

    // All slots (including empty)
    const allSlots = replay.slots()
    console.log(`\nTotal Slots: ${allSlots.length}`)
    console.log(`Empty Slots: ${allSlots.filter(p => p.isEmpty).length}`)
  } catch (error) {
    console.error('✗ Failed to parse replay:', error)
  }
}

/**
 * Example 3: Error handling patterns
 */
function errorHandlingExample() {
  console.log('\n=== Error Handling Example ===')

  // Test with invalid data
  console.log('Testing with invalid data...')
  const invalidData = new Uint8Array([1, 2, 3, 4, 5])

  try {
    const result = parseReplay(invalidData)
    console.log('Unexpected success:', result)
  } catch (error) {
    console.log('✓ Expected error caught:', error.toString())
  }

  // Test with empty data
  console.log('\nTesting with empty data...')
  const emptyData = new Uint8Array(0)

  try {
    const result = parseReplay(emptyData)
    console.log('Unexpected success:', result)
  } catch (error) {
    console.log('✓ Expected error caught:', error.toString())
  }
}

/**
 * Example 4: Custom decompression options
 */
function customOptionsExample() {
  console.log('\n=== Custom Decompression Options Example ===')

  const testReplayPath = path.join(
    import.meta.dirname,
    '..',
    '..',
    'broodrep',
    'testdata',
    'things.rep',
  )

  if (!fs.existsSync(testReplayPath)) {
    console.log('Test replay not found, skipping custom options example...')
    return
  }

  try {
    const replayData = fs.readFileSync(testReplayPath)
    const uint8Array = new Uint8Array(replayData)

    // Create custom decompression options
    const options = {
      maxDecompressedSize: 200 * 1024 * 1024, // 200MB instead of default 100MB
      maxCompressionRatio: 1000.0, // Allow higher compression ratios
    }

    console.log('Custom options configured:')
    console.log(`  Max decompressed size: ${options.maxDecompressedSize || 'default'} bytes`)
    console.log(`  Max compression ratio: ${options.maxCompressionRatio || 'default'}:1`)

    // Parse with custom options
    const replay = parseReplay(uint8Array, options)
    const header = replay.header

    console.log('✓ Successfully parsed replay with custom options!')
    console.log(`  Game: ${header.title}`)
    console.log(`  Players: ${replay.players().filter(p => !p.isEmpty && !p.isObserver).length}`)

    // Compare with default parsing
    const replayDefault = parseReplay(uint8Array) // No options = use defaults
    console.log('✓ Default options also work fine for this replay')
  } catch (error) {
    console.error('✗ Custom options test failed:', error)
  }
}

console.log('broodrep-wasm Usage Examples')
console.log('============================')

basicExample()
detailedExample()
errorHandlingExample()
customOptionsExample()

console.log('\n✓ All examples completed!')
console.log('\nTip: To use in your project:')
console.log('  npm install @shieldbattery/broodrep')
console.log('  import { parseReplay } from "@shieldbattery/broodrep"')
