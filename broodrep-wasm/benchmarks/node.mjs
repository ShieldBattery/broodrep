import { readFileSync } from 'node:fs';
import { createRequire } from 'node:module';
import { basename, resolve } from 'node:path';
import { performance } from 'node:perf_hooks';

const require = createRequire(import.meta.url);
const broodrep = require('../pkg-node/broodrep_wasm.js');

const warmupIterations = 5;
const measuredIterations = 25;
let checksum = 0;

const defaultReplays = [
  resolve(import.meta.dirname, '../../broodrep/testdata/legacy_cp949.rep'),
  resolve(import.meta.dirname, '../../broodrep/testdata/scr_mapname32.rep'),
  resolve(import.meta.dirname, '../../broodrep/testdata/long_hunters.rep'),
];
const replayPaths = process.argv.slice(2).map((path) => resolve(path));

function consume(value) {
  checksum = (checksum + Number(value)) >>> 0;
}

function benchmark(name, operation) {
  for (let iteration = 0; iteration < warmupIterations; iteration += 1) {
    consume(operation());
  }

  const startedAt = performance.now();
  for (let iteration = 0; iteration < measuredIterations; iteration += 1) {
    consume(operation());
  }
  const elapsedMs = performance.now() - startedAt;
  const meanMs = elapsedMs / measuredIterations;
  const opsPerSecond = 1_000 / meanMs;

  console.log(
    `  ${name.padEnd(43)} ${opsPerSecond.toFixed(1).padStart(10)} ops/s  ${meanMs
      .toFixed(3)
      .padStart(8)} ms mean`,
  );

  globalThis.gc?.();
}

function parseReplay(data) {
  return broodrep.parseReplay(data);
}

function commandSummary(replay) {
  const summary = replay.getCommandSummary();
  return summary === undefined ? 0 : summary.total + summary.counts.length;
}

const playerZeroQuery = { playerIds: [0] };
const productionQuery = { includeCategories: ['production'] };

function commandsLength(replay) {
  const commands = replay.getCommands();
  return commands === undefined ? 0 : commands.length;
}

function benchmarkReplay(path) {
  const data = new Uint8Array(readFileSync(path));
  console.log(`\n${basename(path)} (${data.byteLength.toLocaleString()} bytes)`);

  benchmark('parseReplayMetadata', () => {
    const metadata = broodrep.parseReplayMetadata(data);
    return metadata.header.frames + metadata.slots.length;
  });

  benchmark('parseReplay (Replay.free)', () => {
    const replay = parseReplay(data);
    try {
      return replay.header.frames;
    } finally {
      replay.free();
    }
  });

  const summaryReplay = parseReplay(data);
  try {
    benchmark('Replay.getCommandSummary', () => commandSummary(summaryReplay));
    benchmark('Replay.getPlayerApm', () => {
      const summary = summaryReplay.getPlayerApm();
      return summary === undefined ? 0 : summary.players.length;
    });
    benchmark('Replay.queryCommands (player 0)', () =>
      summaryReplay.queryCommands(playerZeroQuery).length,
    );
    benchmark('Replay.queryCommands (production)', () =>
      summaryReplay.queryCommands(productionQuery).length,
    );
  } finally {
    summaryReplay.free();
  }

  const legacyRawReplay = parseReplay(data);
  try {
    benchmark('Replay.getRawSection (full JS copy)', () => {
      const section = legacyRawReplay.getRawSection('commands');
      return section === undefined ? 0 : section.byteLength;
    });
  } finally {
    legacyRawReplay.free();
  }

  const loadRawReplay = parseReplay(data);
  try {
    benchmark('Replay.loadRawSection + RawSection.free', () => {
      const section = loadRawReplay.loadRawSection('commands');
      if (section === undefined) {
        return 0;
      }
      try {
        return section.length;
      } finally {
        section.free();
      }
    });

    const rawSection = loadRawReplay.loadRawSection('commands');
    if (rawSection !== undefined) {
      try {
        const pageSize = Math.min(4096, rawSection.length);
        benchmark('RawSection.copyRange (4 KiB page)', () =>
          rawSection.copyRange(0, pageSize).byteLength,
        );
      } finally {
        rawSection.free();
      }
    }
  } finally {
    loadRawReplay.free();
  }

  const legacyCommandsReplay = parseReplay(data);
  try {
    benchmark('Replay.getCommands (full JS export)', () =>
      commandsLength(legacyCommandsReplay),
    );
  } finally {
    legacyCommandsReplay.free();
  }

  const loadCommandsReplay = parseReplay(data);
  try {
    benchmark('Replay.loadCommands + ParsedCommands.free', () => {
      const commands = loadCommandsReplay.loadCommands();
      if (commands === undefined) {
        return 0;
      }
      try {
        return commands.length;
      } finally {
        commands.free();
      }
    });
  } finally {
    loadCommandsReplay.free();
  }

  const pageReplay = parseReplay(data);
  let parsedCommands;
  try {
    parsedCommands = pageReplay.loadCommands();
    if (parsedCommands === undefined) {
      console.log('  ParsedCommands.getRange (256-command page)   skipped (no command section)');
    } else {
      const pageSize = Math.min(256, parsedCommands.length);
      benchmark('ParsedCommands.getRange (256-command page)', () => {
        const page = parsedCommands.getRange(0, pageSize);
        return page.length;
      });
      benchmark('ParsedCommands.query (player 0)', () =>
        parsedCommands.query(playerZeroQuery).length,
      );
      benchmark('ParsedCommands.getPlayerApm', () =>
        parsedCommands.getPlayerApm().players.length,
      );
    }
  } finally {
    parsedCommands?.free();
    pageReplay.free();
  }
}

broodrep.init?.();

for (const path of replayPaths.length === 0 ? defaultReplays : replayPaths) {
  benchmarkReplay(path);
}

console.log(`\nChecksum: ${checksum}`);
