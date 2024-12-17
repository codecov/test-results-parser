mod bindings;
mod commithashes_set;
mod error;
mod flags_set;
mod format;
mod raw;
mod timestamps;
mod writer;

pub use bindings::{AggregationReader, BinaryFormatWriter, TestAggregate};
pub use error::{TestAnalyticsError, TestAnalyticsErrorKind};
pub use format::{Test, TestAnalytics};
pub use raw::CommitHash;
pub use writer::TestAnalyticsWriter;

#[cfg(test)]
mod tests {
    use raw::CommitHash;
    use timestamps::DAY;

    use crate::testrun::{Outcome, Testrun};

    use super::*;

    fn test() -> Testrun {
        Testrun {
            name: "abc".into(),
            classname: "".into(),
            duration: Some(1.0),
            outcome: Outcome::Pass,
            testsuite: "".into(),
            failure_message: None,
            filename: None,
            build_url: None,
            computed_name: None,
        }
    }

    #[test]
    fn test_empty() {
        let writer = TestAnalyticsWriter::new(60);

        let mut buf = vec![];
        writer.serialize(&mut buf).unwrap();

        let parsed = TestAnalytics::parse(&buf, 0).unwrap();
        assert!(parsed.tests(0..60, None).unwrap().next().is_none());
    }

    #[test]
    fn test_builder() {
        let mut test = test();

        let mut writer = TestAnalyticsWriter::new(2);
        let mut session = writer.start_session(0, CommitHash::default(), &[]);

        session.insert(&test);

        test.outcome = Outcome::Failure;
        test.duration = Some(2.0);
        session.insert(&test);

        test.name = "def".into();
        test.outcome = Outcome::Skip;
        test.duration = Some(0.0);
        session.insert(&test);

        let mut buf = vec![];
        writer.serialize(&mut buf).unwrap();

        let parsed = TestAnalytics::parse(&buf, 0).unwrap();
        let mut tests = parsed.tests(0..60, None).unwrap();

        let abc = tests.next().unwrap().unwrap();
        assert_eq!(abc.name().unwrap(), "abc");
        let aggregates = abc.aggregates();
        assert_eq!(aggregates.total_pass_count, 1);
        assert_eq!(aggregates.total_fail_count, 1);
        assert_eq!(aggregates.avg_duration, 1.5);

        let abc = tests.next().unwrap().unwrap();
        assert_eq!(abc.name().unwrap(), "def");
        let aggregates = abc.aggregates();
        assert_eq!(aggregates.total_skip_count, 1);

        assert!(tests.next().is_none());
    }

    #[test]
    fn test_testsuites() {
        let mut test = test();

        let mut writer = TestAnalyticsWriter::new(2);
        let mut session = writer.start_session(0, CommitHash::default(), &[]);

        session.insert(&test);
        test.testsuite = "some testsuite".into();
        session.insert(&test);

        let mut buf = vec![];
        writer.serialize(&mut buf).unwrap();

        let parsed = TestAnalytics::parse(&buf, 0).unwrap();
        let mut tests = parsed.tests(0..60, None).unwrap();

        let abc = tests.next().unwrap().unwrap();
        assert_eq!(abc.testsuite().unwrap(), "");
        assert_eq!(abc.name().unwrap(), "abc");

        let abc_with_testsuite = tests.next().unwrap().unwrap();
        assert_eq!(abc_with_testsuite.testsuite().unwrap(), "some testsuite");
        assert_eq!(abc_with_testsuite.name().unwrap(), "abc");

        assert!(tests.next().is_none());
    }

    #[test]
    fn test_time_shift() {
        let test = test();

        let mut writer = TestAnalyticsWriter::new(2);
        let mut session = writer.start_session(0, CommitHash::default(), &[]);

        session.insert(&test);

        let mut buf = vec![];
        writer.serialize(&mut buf).unwrap();

        // the test was written at timestamp `0`, and we parse at that same timestamp
        // so we expect the data in the "today" bucket
        let parsed = TestAnalytics::parse(&buf, 0).unwrap();
        let mut tests = parsed.tests(0..1, None).unwrap();

        let abc = tests.next().unwrap().unwrap();
        assert_eq!(abc.name().unwrap(), "abc");
        let aggregates = abc.aggregates();
        assert_eq!(aggregates.total_pass_count, 1);
        assert_eq!(aggregates.avg_duration, 1.0);

        assert!(tests.next().is_none());

        // next, we re-parse one day ahead
        let parsed = TestAnalytics::parse(&buf, DAY).unwrap();

        // the test has no data for "today", so is not being yielded
        let mut tests = parsed.tests(0..1, None).unwrap();
        assert!(tests.next().is_none());

        // the data should be in the "yesterday" bucket
        let mut tests = parsed.tests(1..2, None).unwrap();

        let abc = tests.next().unwrap().unwrap();
        assert_eq!(abc.name().unwrap(), "abc");
        let aggregates = abc.aggregates();
        assert_eq!(aggregates.total_pass_count, 1);
        assert_eq!(aggregates.avg_duration, 1.0);

        assert!(tests.next().is_none());
    }

    #[test]
    fn test_append_data() {
        let test = test();

        let mut writer = TestAnalyticsWriter::new(2);
        let mut session = writer.start_session(0, CommitHash::default(), &[]);

        session.insert(&test);

        let mut buf = vec![];
        writer.serialize(&mut buf).unwrap();

        let parsed = TestAnalytics::parse(&buf, DAY).unwrap();
        let mut writer = TestAnalyticsWriter::from_existing_format(&parsed).unwrap();
        let mut session = writer.start_session(DAY, CommitHash::default(), &[]);

        session.insert(&test);

        let mut buf = vec![];
        writer.serialize(&mut buf).unwrap();

        let parsed = TestAnalytics::parse(&buf, DAY).unwrap();

        // we should have data in the "today" bucket
        let mut tests = parsed.tests(0..1, None).unwrap();
        let abc = tests.next().unwrap().unwrap();
        assert_eq!(abc.name().unwrap(), "abc");
        let aggregates = abc.aggregates();
        assert_eq!(aggregates.total_pass_count, 1);
        assert_eq!(aggregates.avg_duration, 1.0);
        assert!(tests.next().is_none());

        // as well as in the "yesterday" bucket
        let mut tests = parsed.tests(1..2, None).unwrap();
        let abc = tests.next().unwrap().unwrap();
        assert_eq!(abc.name().unwrap(), "abc");
        let aggregates = abc.aggregates();
        assert_eq!(aggregates.total_pass_count, 1);
        assert_eq!(aggregates.avg_duration, 1.0);
        assert!(tests.next().is_none());
    }

    #[test]
    fn test_merge() {
        let test = test();

        let mut writer = TestAnalyticsWriter::new(2);
        let mut session = writer.start_session(0, CommitHash::default(), &[]);
        session.insert(&test);
        let mut buf_1 = vec![];
        writer.serialize(&mut buf_1).unwrap();

        let mut writer = TestAnalyticsWriter::new(2);
        let mut session = writer.start_session(DAY, CommitHash::default(), &[]);
        session.insert(&test);
        let mut buf_2 = vec![];
        writer.serialize(&mut buf_2).unwrap();

        let parsed_1 = TestAnalytics::parse(&buf_1, DAY).unwrap();
        let parsed_2 = TestAnalytics::parse(&buf_2, DAY).unwrap();

        let merged_12 = TestAnalyticsWriter::merge(&parsed_1, &parsed_2).unwrap();
        let merged_21 = TestAnalyticsWriter::merge(&parsed_2, &parsed_1).unwrap();

        let mut buf_12 = vec![];
        merged_12.serialize(&mut buf_12).unwrap();
        let mut buf_21 = vec![];
        merged_21.serialize(&mut buf_21).unwrap();

        assert_eq!(buf_12, buf_21);

        let parsed = TestAnalytics::parse(&buf_12, DAY).unwrap();

        // we should have data in the "today" bucket
        let mut tests = parsed.tests(0..1, None).unwrap();
        let abc = tests.next().unwrap().unwrap();
        assert_eq!(abc.name().unwrap(), "abc");
        let aggregates = abc.aggregates();
        assert_eq!(aggregates.total_pass_count, 1);
        assert_eq!(aggregates.avg_duration, 1.0);
        assert!(tests.next().is_none());

        // as well as in the "yesterday" bucket
        let mut tests = parsed.tests(1..2, None).unwrap();
        let abc = tests.next().unwrap().unwrap();
        assert_eq!(abc.name().unwrap(), "abc");
        let aggregates = abc.aggregates();
        assert_eq!(aggregates.total_pass_count, 1);
        assert_eq!(aggregates.avg_duration, 1.0);
        assert!(tests.next().is_none());
    }

    #[test]
    fn test_garbage_collection() {
        let test = test();

        let mut writer = TestAnalyticsWriter::new(2);
        let mut session = writer.start_session(0, CommitHash::default(), &[]);

        session.insert(&test);

        let mut buf = vec![];
        writer.serialize(&mut buf).unwrap();

        let parsed = TestAnalytics::parse(&buf, DAY).unwrap();
        let mut writer = TestAnalyticsWriter::from_existing_format(&parsed).unwrap();

        let was_rewritten = writer.rewrite(2, DAY, Some(0)).unwrap();
        assert!(!was_rewritten);

        let was_rewritten = writer.rewrite(7, DAY, Some(0)).unwrap();
        assert!(was_rewritten);

        let mut buf = vec![];
        writer.serialize(&mut buf).unwrap();

        let parsed = TestAnalytics::parse(&buf, DAY).unwrap();

        // nothing garbage collected yet,
        // we should have data in the "yesterday" bucket
        let mut tests = parsed.tests(1..2, None).unwrap();
        let abc = tests.next().unwrap().unwrap();
        assert_eq!(abc.name().unwrap(), "abc");
        let aggregates = abc.aggregates();
        assert_eq!(aggregates.total_pass_count, 1);
        assert_eq!(aggregates.avg_duration, 1.0);
        assert!(tests.next().is_none());

        let mut writer = TestAnalyticsWriter::from_existing_format(&parsed).unwrap();

        let was_rewritten = writer.rewrite(2, 3 * DAY, Some(0)).unwrap();
        assert!(was_rewritten);

        let mut buf = vec![];
        writer.serialize(&mut buf).unwrap();

        let parsed = TestAnalytics::parse(&buf, 3 * DAY).unwrap();
        let mut tests = parsed.tests(0..60, None).unwrap();

        // the test was garbage collected
        assert!(tests.next().is_none());
    }

    #[test]
    fn test_flags() {
        let test = test();

        let mut writer = TestAnalyticsWriter::new(2);

        let mut session = writer.start_session(0, CommitHash::default(), &["flag-a"]);
        session.insert(&test);
        let mut session = writer.start_session(0, CommitHash::default(), &["flag-b"]);
        session.insert(&test);

        let mut buf = vec![];
        writer.serialize(&mut buf).unwrap();

        let parsed = TestAnalytics::parse(&buf, DAY).unwrap();
        let mut tests = parsed.tests(0..60, None).unwrap();

        // we get the test twice, with two different flags
        let abc = tests.next().unwrap().unwrap();
        assert_eq!(abc.name().unwrap(), "abc");
        assert_eq!(abc.flags().unwrap(), &["flag-a"]);

        let abc = tests.next().unwrap().unwrap();
        assert_eq!(abc.name().unwrap(), "abc");
        assert_eq!(abc.flags().unwrap(), &["flag-b"]);

        assert!(tests.next().is_none());

        // if we filter for flags, we get only matching tests:
        let mut tests = parsed.tests(0..60, Some(&["flag-a"])).unwrap();

        let abc = tests.next().unwrap().unwrap();
        assert_eq!(abc.name().unwrap(), "abc");
        assert_eq!(abc.flags().unwrap(), &["flag-a"]);
        assert!(tests.next().is_none());

        let mut tests = parsed.tests(0..60, Some(&["non-existing"])).unwrap();
        assert!(tests.next().is_none());
    }

    #[test]
    fn test_historic_data() {
        let test = test();

        let mut writer = TestAnalyticsWriter::new(7);

        let mut session = writer.start_session(3 * DAY, CommitHash::default(), &[]);
        session.insert(&test);
        // insert data older than what is already in the file
        let mut session = writer.start_session(DAY, CommitHash::default(), &[]);
        session.insert(&test);

        let mut buf = vec![];
        writer.serialize(&mut buf).unwrap();

        let parsed = TestAnalytics::parse(&buf, 4 * DAY).unwrap();

        // we do not have any test data for "today"
        let mut tests = parsed.tests(0..1, None).unwrap();
        assert!(tests.next().is_none());

        // when filtering for "yesterday", we get valid data
        let mut tests = parsed.tests(1..2, None).unwrap();
        let abc = tests.next().unwrap().unwrap();
        assert_eq!(abc.name().unwrap(), "abc");
        assert!(tests.next().is_none());

        // also when filtering for two days prior to that
        let mut tests = parsed.tests(2..4, None).unwrap();
        let abc = tests.next().unwrap().unwrap();
        assert_eq!(abc.name().unwrap(), "abc");
        assert!(tests.next().is_none());

        // but not when going further back in time
        let mut tests = parsed.tests(5..7, None).unwrap();
        assert!(tests.next().is_none());
    }

    #[test]
    fn test_commit_hashes() {
        let mut test = test();
        test.outcome = Outcome::Failure;
        let commit_1 = CommitHash([0; 20]);
        let commit_2 = CommitHash([1; 20]);
        let commit_3 = CommitHash([2; 20]);
        let commit_4 = CommitHash([3; 20]);

        let mut writer = TestAnalyticsWriter::new(7);

        let mut session = writer.start_session(DAY, commit_1, &[]);
        session.insert(&test);

        let mut buf1 = vec![];
        writer.serialize(&mut buf1).unwrap();

        let mut writer = TestAnalyticsWriter::new(7);

        let mut session = writer.start_session(DAY, commit_2, &[]);
        session.insert(&test);
        let mut session = writer.start_session(2 * DAY, commit_3, &[]);
        session.insert(&test);
        let mut session = writer.start_session(3 * DAY, commit_4, &[]);
        session.insert(&test);

        let mut buf2 = vec![];
        writer.serialize(&mut buf2).unwrap();

        let parsed1 = TestAnalytics::parse(&buf1, 3 * DAY).unwrap();
        let parsed2 = TestAnalytics::parse(&buf2, 3 * DAY).unwrap();

        let merged = TestAnalyticsWriter::merge(&parsed1, &parsed2).unwrap();
        let mut buf = vec![];
        merged.serialize(&mut buf).unwrap();

        let parsed = TestAnalytics::parse(&buf, 3 * DAY).unwrap();
        let mut writer = TestAnalyticsWriter::from_existing_format(&parsed).unwrap();
        let was_rewritten = writer.rewrite(14, 3 * DAY, Some(0)).unwrap();
        assert!(was_rewritten);
        let mut buf = vec![];
        writer.serialize(&mut buf).unwrap();

        let parsed = TestAnalytics::parse(&buf, 3 * DAY).unwrap();

        let mut tests = parsed.tests(0..1, None).unwrap();
        let abc = tests.next().unwrap().unwrap();
        assert_eq!(abc.aggregates().failing_commits, 1); // commit 4
        assert!(tests.next().is_none());

        let mut tests = parsed.tests(2..3, None).unwrap();
        let abc = tests.next().unwrap().unwrap();
        assert_eq!(abc.aggregates().failing_commits, 2); // commit 1, commit 2
        assert!(tests.next().is_none());

        let mut tests = parsed.tests(0..60, None).unwrap();
        let abc = tests.next().unwrap().unwrap();
        assert_eq!(abc.aggregates().failing_commits, 4); // commit 1 - 4
        assert!(tests.next().is_none());
    }
}
