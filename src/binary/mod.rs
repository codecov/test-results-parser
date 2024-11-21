mod error;
mod format;
mod raw;
mod timestamps;
mod writer;

pub use error::{TestAnalyticsError, TestAnalyticsErrorKind};
pub use format::TestAnalytics;
pub use writer::TestAnalyticsWriter;

#[cfg(test)]
mod tests {
    use timestamps::DAY;

    use crate::testrun::{Outcome, Testrun};

    use super::*;

    #[test]
    fn test_empty() {
        let writer = TestAnalyticsWriter::new(60, 0);

        let mut buf = vec![];
        writer.serialize(&mut buf).unwrap();

        let parsed = TestAnalytics::parse(&buf, 0).unwrap();
        assert!(parsed.tests().next().is_none());
    }

    #[test]
    fn test_builder() {
        let mut test = Testrun {
            name: "abc".into(),
            classname: "".into(),
            duration: 1.0,
            outcome: Outcome::Pass,
            testsuite: "".into(),
            failure_message: None,
            filename: None,
            build_url: None,
            computed_name: None,
        };

        let mut writer = TestAnalyticsWriter::new(2, 0);

        writer.add_test_run(&test);

        test.outcome = Outcome::Failure;
        test.duration = 2.0;
        writer.add_test_run(&test);

        test.name = "def".into();
        test.outcome = Outcome::Skip;
        test.duration = 0.0;
        writer.add_test_run(&test);

        let mut buf = vec![];
        writer.serialize(&mut buf).unwrap();

        let parsed = TestAnalytics::parse(&buf, 0).unwrap();
        let mut tests = parsed.tests();

        let abc = tests.next().unwrap();
        assert_eq!(abc.name().unwrap(), "abc");
        let aggregates = abc.get_aggregates(0..60);
        assert_eq!(aggregates.total_pass_count, 1);
        assert_eq!(aggregates.total_fail_count, 1);
        assert_eq!(aggregates.avg_duration, 1.5);

        let abc = tests.next().unwrap();
        assert_eq!(abc.name().unwrap(), "def");
        let aggregates = abc.get_aggregates(0..60);
        assert_eq!(aggregates.total_skip_count, 1);

        assert!(tests.next().is_none());
    }

    #[test]
    fn test_time_shift() {
        let test = Testrun {
            name: "abc".into(),
            classname: "".into(),
            duration: 1.0,
            outcome: Outcome::Pass,
            testsuite: "".into(),
            failure_message: None,
            filename: None,
            build_url: None,
            computed_name: None,
        };

        let mut writer = TestAnalyticsWriter::new(2, 0);

        writer.add_test_run(&test);

        let mut buf = vec![];
        writer.serialize(&mut buf).unwrap();

        // the test was written at timestamp `0`, and we parse at that same timestamp
        // so we expect the data in the "today" bucket
        let parsed = TestAnalytics::parse(&buf, 0).unwrap();
        let mut tests = parsed.tests();

        let abc = tests.next().unwrap();
        assert_eq!(abc.name().unwrap(), "abc");
        let aggregates = abc.get_aggregates(0..1);
        assert_eq!(aggregates.total_pass_count, 1);
        assert_eq!(aggregates.avg_duration, 1.0);

        assert!(tests.next().is_none());

        // next, we re-parse one day ahead
        let parsed = TestAnalytics::parse(&buf, 1 * DAY).unwrap();
        let mut tests = parsed.tests();

        let abc = tests.next().unwrap();
        assert_eq!(abc.name().unwrap(), "abc");

        // the "today" bucket should be empty
        let aggregates = abc.get_aggregates(0..1);
        assert_eq!(aggregates.total_pass_count, 0);

        // now, the data should be in the "yesterday" bucket
        let aggregates = abc.get_aggregates(1..2);
        assert_eq!(aggregates.total_pass_count, 1);
        assert_eq!(aggregates.avg_duration, 1.0);

        assert!(tests.next().is_none());
    }

    #[test]
    fn test_append_data() {
        let test = Testrun {
            name: "abc".into(),
            classname: "".into(),
            duration: 1.0,
            outcome: Outcome::Pass,
            testsuite: "".into(),
            failure_message: None,
            filename: None,
            build_url: None,
            computed_name: None,
        };

        let mut writer = TestAnalyticsWriter::new(2, 0);

        writer.add_test_run(&test);

        let mut buf = vec![];
        writer.serialize(&mut buf).unwrap();

        let parsed = TestAnalytics::parse(&buf, 1 * DAY).unwrap();
        let mut writer = TestAnalyticsWriter::from_existing_format(&parsed, 1 * DAY).unwrap();

        writer.add_test_run(&test);

        let mut buf = vec![];
        writer.serialize(&mut buf).unwrap();

        let parsed = TestAnalytics::parse(&buf, 1 * DAY).unwrap();
        let mut tests = parsed.tests();

        let abc = tests.next().unwrap();
        assert_eq!(abc.name().unwrap(), "abc");

        // we should have data in the "today" bucket
        let aggregates = abc.get_aggregates(0..1);
        assert_eq!(aggregates.total_pass_count, 1);
        assert_eq!(aggregates.avg_duration, 1.0);

        // as well as in the "yesterday" bucket
        let aggregates = abc.get_aggregates(1..2);
        assert_eq!(aggregates.total_pass_count, 1);
        assert_eq!(aggregates.avg_duration, 1.0);

        assert!(tests.next().is_none());
    }

    #[test]
    fn test_merge() {
        let test = Testrun {
            name: "abc".into(),
            classname: "".into(),
            duration: 1.0,
            outcome: Outcome::Pass,
            testsuite: "".into(),
            failure_message: None,
            filename: None,
            build_url: None,
            computed_name: None,
        };

        let mut writer = TestAnalyticsWriter::new(2, 0);
        writer.add_test_run(&test);
        let mut buf_1 = vec![];
        writer.serialize(&mut buf_1).unwrap();

        let mut writer = TestAnalyticsWriter::new(2, 1 * DAY);
        writer.add_test_run(&test);
        let mut buf_2 = vec![];
        writer.serialize(&mut buf_2).unwrap();

        let parsed_1 = TestAnalytics::parse(&buf_1, 1 * DAY).unwrap();
        let parsed_2 = TestAnalytics::parse(&buf_2, 1 * DAY).unwrap();

        let merged_12 = TestAnalyticsWriter::merge(&parsed_1, &parsed_2, 1 * DAY).unwrap();
        let merged_21 = TestAnalyticsWriter::merge(&parsed_2, &parsed_1, 1 * DAY).unwrap();

        let mut buf_12 = vec![];
        merged_12.serialize(&mut buf_12).unwrap();
        let mut buf_21 = vec![];
        merged_21.serialize(&mut buf_21).unwrap();

        assert_eq!(buf_12, buf_21);

        let parsed = TestAnalytics::parse(&buf_12, 1 * DAY).unwrap();
        let mut tests = parsed.tests();

        let abc = tests.next().unwrap();
        assert_eq!(abc.name().unwrap(), "abc");

        // we should have data in the "today" bucket
        let aggregates = abc.get_aggregates(0..1);
        assert_eq!(aggregates.total_pass_count, 1);
        assert_eq!(aggregates.avg_duration, 1.0);

        // as well as in the "yesterday" bucket
        let aggregates = abc.get_aggregates(1..2);
        assert_eq!(aggregates.total_pass_count, 1);
        assert_eq!(aggregates.avg_duration, 1.0);

        assert!(tests.next().is_none());
    }
}
