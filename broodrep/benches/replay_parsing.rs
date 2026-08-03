use std::{fs::File, hint::black_box, io::Cursor, ops::ControlFlow, path::Path};

use broodrep::{
    Replay, ReplaySection, TextEncoding,
    commands::{parse_commands, visit_commands},
};
use criterion::{BatchSize, BenchmarkId, Criterion, Throughput, criterion_group, criterion_main};

// These cover both compression formats plus a longer, command-heavy multiplayer replay.
const LEGACY: &[u8] = include_bytes!("../testdata/legacy_cp949.rep");
const MODERN_121: &[u8] = include_bytes!("../testdata/scr_mapname32.rep");
const LONG_MULTIPLAYER: &[u8] = include_bytes!("../testdata/long_hunters.rep");

struct Fixture {
    id: &'static str,
    replay: &'static [u8],
    path: &'static str,
    encoding: TextEncoding,
}

const FIXTURES: &[Fixture] = &[
    Fixture {
        id: "legacy_cp949",
        replay: LEGACY,
        path: "testdata/legacy_cp949.rep",
        encoding: TextEncoding::Legacy,
    },
    Fixture {
        id: "modern121_mapname32",
        replay: MODERN_121,
        path: "testdata/scr_mapname32.rep",
        encoding: TextEncoding::Utf8,
    },
    Fixture {
        id: "modern121_long_hunters",
        replay: LONG_MULTIPLAYER,
        path: "testdata/long_hunters.rep",
        encoding: TextEncoding::Utf8,
    },
];

fn command_bytes(fixture: &Fixture) -> Vec<u8> {
    let mut replay = Replay::new(Cursor::new(fixture.replay)).expect("fixture is a valid replay");
    replay
        .get_raw_section(ReplaySection::Commands)
        .expect("commands section can be read")
        .expect("fixture has a commands section")
}

fn replay_new(c: &mut Criterion) {
    let mut group = c.benchmark_group("replay_new");

    for fixture in FIXTURES {
        group.throughput(Throughput::Bytes(fixture.replay.len() as u64));
        group.bench_with_input(
            BenchmarkId::new("header_index", fixture.id),
            fixture,
            |b, fixture| {
                b.iter_batched(
                    || Cursor::new(fixture.replay),
                    |reader| black_box(Replay::new(reader).expect("fixture is a valid replay")),
                    BatchSize::SmallInput,
                );
            },
        );

        let path = Path::new(env!("CARGO_MANIFEST_DIR")).join(fixture.path);
        let mut file = File::open(path).expect("fixture can be opened");
        group.bench_function(BenchmarkId::new("header_index_file", fixture.id), |b| {
            b.iter(|| {
                let replay = Replay::new(&mut file).expect("fixture is a valid replay");
                black_box(replay.format());
                drop(replay);
            });
        });
    }

    group.finish();
}

fn raw_commands(c: &mut Criterion) {
    let mut group = c.benchmark_group("raw_commands");

    for fixture in FIXTURES {
        let command_bytes = command_bytes(fixture);

        group.throughput(Throughput::Bytes(command_bytes.len() as u64));
        group.bench_function(BenchmarkId::new("indexed_extract", fixture.id), |b| {
            let mut replay =
                Replay::new(Cursor::new(fixture.replay)).expect("fixture is a valid replay");
            b.iter(|| {
                black_box(
                    replay
                        .get_raw_section(ReplaySection::Commands)
                        .expect("commands section can be read")
                        .expect("fixture has a commands section"),
                )
            });
        });
        group.bench_with_input(
            BenchmarkId::new("cold_new_and_extract", fixture.id),
            fixture,
            |b, fixture| {
                b.iter_batched(
                    || Cursor::new(fixture.replay),
                    |reader| {
                        let mut replay = Replay::new(reader).expect("fixture is a valid replay");
                        black_box(
                            replay
                                .read_raw_section(ReplaySection::Commands)
                                .expect("commands section can be read")
                                .expect("fixture has a commands section"),
                        )
                    },
                    BatchSize::SmallInput,
                );
            },
        );
    }

    group.finish();
}

fn parse_fixture_commands(c: &mut Criterion) {
    let mut group = c.benchmark_group("parse_commands");

    for fixture in FIXTURES {
        let command_bytes = command_bytes(fixture);
        group.throughput(Throughput::Bytes(command_bytes.len() as u64));
        let input = (command_bytes, fixture.encoding);
        group.bench_with_input(
            BenchmarkId::new("fixture", fixture.id),
            &input,
            |b, (command_bytes, encoding)| {
                b.iter(|| {
                    black_box(
                        parse_commands(black_box(command_bytes), *encoding)
                            .expect("fixture commands are valid"),
                    )
                });
            },
        );
        group.bench_with_input(
            BenchmarkId::new("visit_fixture", fixture.id),
            &input,
            |b, (command_bytes, encoding)| {
                b.iter(|| {
                    let mut count = 0usize;
                    let outcome = visit_commands(black_box(command_bytes), *encoding, |command| {
                        black_box(command);
                        count += 1;
                        ControlFlow::Continue(())
                    })
                    .expect("fixture commands are valid");
                    black_box((outcome, count))
                });
            },
        );
    }

    group.finish();
}

fn replay_with_commands(c: &mut Criterion) {
    let mut group = c.benchmark_group("replay_with_commands");

    for fixture in FIXTURES {
        group.throughput(Throughput::Bytes(fixture.replay.len() as u64));
        group.bench_with_input(
            BenchmarkId::new("new_and_parse", fixture.id),
            fixture,
            |b, fixture| {
                b.iter_batched(
                    || Cursor::new(fixture.replay),
                    |reader| {
                        let mut replay = Replay::new(reader).expect("fixture is a valid replay");
                        black_box(
                            replay
                                .get_commands()
                                .expect("commands can be parsed")
                                .expect("fixture has a commands section"),
                        )
                    },
                    BatchSize::SmallInput,
                );
            },
        );
    }

    group.finish();
}

fn dense_keepalive_stream() -> Vec<u8> {
    const TARGET_LEN: usize = 1024 * 1024;
    const BLOCKS: usize = 4050;
    const FULL_BLOCK_COMMANDS: usize = 127;

    // Each frame block is 5 bytes of framing plus two bytes per KeepAlive command. 4,048 full
    // blocks, one 66-command block, and one single-command block total exactly 1 MiB.
    let mut data = Vec::with_capacity(TARGET_LEN);
    for block in 0..BLOCKS {
        let command_count = match block {
            0..4048 => FULL_BLOCK_COMMANDS,
            4048 => 66,
            4049 => 1,
            _ => unreachable!(),
        };
        data.extend_from_slice(&(block as u32).to_le_bytes());
        data.push((command_count * 2) as u8);
        for _ in 0..command_count {
            data.extend_from_slice(&[0, 0x05]);
        }
    }
    assert_eq!(data.len(), TARGET_LEN);
    data
}

fn parse_dense_keepalives(c: &mut Criterion) {
    let data = dense_keepalive_stream();
    let mut group = c.benchmark_group("parse_commands");
    group.throughput(Throughput::Bytes(data.len() as u64));
    group.bench_function("dense_keepalive_1_mib", |b| {
        b.iter(|| {
            black_box(
                parse_commands(black_box(&data), TextEncoding::Utf8)
                    .expect("generated commands are valid"),
            )
        });
    });
    group.bench_function("visit_dense_keepalive_1_mib", |b| {
        b.iter(|| {
            let mut count = 0usize;
            let outcome = visit_commands(black_box(&data), TextEncoding::Utf8, |command| {
                black_box(command);
                count += 1;
                ControlFlow::Continue(())
            })
            .expect("generated commands are valid");
            black_box((outcome, count))
        });
    });
    group.finish();
}

criterion_group!(
    benches,
    replay_new,
    raw_commands,
    parse_fixture_commands,
    replay_with_commands,
    parse_dense_keepalives
);
criterion_main!(benches);
