use std::hint::black_box;

use criterion::{criterion_group, criterion_main, Criterion};
use rand::distributions::{Alphanumeric, DistString, Distribution, Uniform, WeightedIndex};
use rand::rngs::SmallRng;
use rand::seq::SliceRandom as _;
use rand::{Rng, SeedableRng};
use test_results_parser::binary::*;
use test_results_parser::{Outcome, Testrun};

criterion_group!(benches, binary);
criterion_main!(benches);

const NUM_TESTS: usize = 100_000;
const NON_OVERLAP: usize = 10_000;
const DAY: u32 = 24 * 60 * 60;

fn binary(c: &mut Criterion) {
    let rand = &mut SmallRng::seed_from_u64(0);

    let mut tests = create_random_testcases(rand, NUM_TESTS);
    randomize_test_data(rand, &mut tests);

    let buf = write_tests(&tests, 60, 0);
    let buf_1 = write_tests(&tests[..NUM_TESTS - NON_OVERLAP], 60, 0);
    randomize_test_data(rand, &mut tests);
    let buf_2 = write_tests(&tests[NON_OVERLAP..], 60, 1 * DAY);

    c.benchmark_group("binary")
        .throughput(criterion::Throughput::Elements(NUM_TESTS as u64))
        .sample_size(10) // because with the configured `NUM_TESTS`, each iteration would run >100ms
        .bench_function("create_and_serialize", |b| {
            b.iter(|| {
                write_tests(&tests, 60, 0);
            })
        })
        .bench_function("read_aggregation", |b| {
            b.iter(|| {
                let parsed = TestAnalytics::parse(&buf, 0).unwrap();
                for test in parsed.tests() {
                    let _name = black_box(test.name().unwrap());
                    let _aggregates = black_box(test.get_aggregates(0..60));
                }
            })
        })
        .bench_function("update_same", |b| {
            b.iter(|| {
                let parsed = TestAnalytics::parse(&buf, 1).unwrap();
                let mut writer = TestAnalyticsWriter::from_existing_format(&parsed).unwrap();
                let mut session = writer.start_session(1, &[]);
                for test in &tests {
                    session.insert(test);
                }

                let mut buf = vec![];
                writer.serialize(&mut buf).unwrap();
                buf
            })
        })
        .bench_function("update_different", |b| {
            b.iter(|| {
                let parsed = TestAnalytics::parse(&buf_1, 1 * DAY).unwrap();
                let mut writer = TestAnalyticsWriter::from_existing_format(&parsed).unwrap();
                let mut session = writer.start_session(1 * DAY, &[]);
                for test in &tests[NON_OVERLAP..] {
                    session.insert(test);
                }

                let mut buf = vec![];
                writer.serialize(&mut buf).unwrap();
                buf
            })
        })
        .bench_function("merge", |b| {
            b.iter(|| {
                let parsed_1 = TestAnalytics::parse(&buf_1, 1 * DAY).unwrap();
                let parsed_2 = TestAnalytics::parse(&buf_2, 1 * DAY).unwrap();
                let writer = TestAnalyticsWriter::merge(&parsed_1, &parsed_2).unwrap();

                let mut buf = vec![];
                writer.serialize(&mut buf).unwrap();
                buf
            })
        })
        .bench_function("merge_rewrite", |b| {
            b.iter(|| {
                let parsed_1 = TestAnalytics::parse(&buf_1, 1 * DAY).unwrap();
                let parsed_2 = TestAnalytics::parse(&buf_2, 1 * DAY).unwrap();
                let mut writer = TestAnalyticsWriter::merge(&parsed_1, &parsed_2).unwrap();

                writer.rewrite(60, 1 * DAY, Some(0)).unwrap();

                let mut buf = vec![];
                writer.serialize(&mut buf).unwrap();
                buf
            })
        });
}

fn write_tests(tests: &[Testrun], num_days: usize, timestamp: u32) -> Vec<u8> {
    let mut writer = TestAnalyticsWriter::new(num_days);
    let mut session = writer.start_session(timestamp, &[]);
    for test in tests {
        session.insert(test);
    }

    let mut buf = vec![];
    writer.serialize(&mut buf).unwrap();
    buf
}

struct Upload {
    flags: Vec<String>,
    tests: Vec<Testrun>,
}

/// Generates a random set of `num_flags` flags.
fn create_random_flags(rng: &mut impl Rng, num_flags: usize) -> Vec<String> {
    let flag_lens = Uniform::from(5usize..10);
    (0..num_flags)
        .map(|_| {
            let flag_len = flag_lens.sample(rng);
            Alphanumeric.sample_string(rng, flag_len)
        })
        .collect()
}

/// Samples random combinations of flags with length `max_flags_in_set`.
fn sample_flag_sets<'a>(
    rng: &'a mut impl Rng,
    flags: &'a [String],
    max_flags_in_set: usize,
) -> impl Iterator<Item = Vec<String>> + 'a {
    let num_flags = Uniform::from(0..max_flags_in_set);
    std::iter::from_fn(move || {
        let num_flags = num_flags.sample(rng);
        let flags: Vec<_> = flags.choose_multiple(rng, num_flags).cloned().collect();
        Some(flags)
    })
}

fn create_random_testcases(rng: &mut impl Rng, num_tests: usize) -> Vec<Testrun> {
    let name_lens = Uniform::from(5usize..50);

    (0..num_tests)
        .map(|_| {
            let name_len = name_lens.sample(rng);
            let name = Alphanumeric.sample_string(rng, name_len);

            Testrun {
                name,
                classname: "".into(),
                duration: 0.,
                outcome: Outcome::Pass,
                testsuite: "".into(),
                failure_message: None,
                filename: None,
                build_url: None,
                computed_name: None,
            }
        })
        .collect()
}

fn randomize_test_data(rng: &mut impl Rng, tests: &mut [Testrun]) {
    let durations = Uniform::from(0f64..10f64);
    let outcomes = WeightedIndex::new([1000, 10, 20]).unwrap();

    for test in tests {
        test.duration = durations.sample(rng);
        test.outcome = match outcomes.sample(rng) {
            0 => Outcome::Pass,
            1 => Outcome::Skip,
            _ => Outcome::Failure,
        };
    }
}
