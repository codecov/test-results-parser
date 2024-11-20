mod error;
mod format;
mod raw;
mod writer;

pub use error::{TestAnalyticsError, TestAnalyticsErrorKind};
pub use format::TestAnalytics;
pub use writer::TestAnalyticsWriter;

#[cfg(test)]
mod tests {
    use crate::testrun::{Outcome, Testrun};

    use super::*;

    #[test]
    fn test_empty() {
        let writer = TestAnalyticsWriter::new(60);

        let mut buf = vec![];
        writer.serialize(&mut buf).unwrap();

        let parsed = TestAnalytics::parse(&buf).unwrap();
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

        let mut writer = TestAnalyticsWriter::new(1);

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

        let parsed = TestAnalytics::parse(&buf).unwrap();
        let mut tests = parsed.tests();

        let abc = tests.next().unwrap().unwrap();
        assert_eq!(abc.name(), "abc");
        let aggregates = abc.get_aggregates(60..0);
        assert_eq!(aggregates.total_pass_count, 1);
        assert_eq!(aggregates.total_fail_count, 1);
        assert_eq!(aggregates.avg_duration, 1.5);

        let abc = tests.next().unwrap().unwrap();
        assert_eq!(abc.name(), "def");
        let aggregates = abc.get_aggregates(60..0);
        assert_eq!(aggregates.total_skip_count, 1);

        assert!(tests.next().is_none());
    }
}
