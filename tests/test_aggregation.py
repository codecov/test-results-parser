from datetime import datetime, timezone

from test_results_parser import (
    parse_junit_xml,
    AggregationReader,
    BinaryFormatWriter,
)

def test_aggregation():
    with open("./tests/junit.xml", "br") as f:
        junit_file = f.read()
    parsed = parse_junit_xml(junit_file)

    now = int(datetime.now(timezone.utc).timestamp())

    writer = BinaryFormatWriter()
    writer.add_testruns(
        timestamp=now,
        commit_hash="e9fcd08652d091fa0c8d28e323c24fb0f4acf249",
        flags=["upload", "flags"],
        testruns=parsed.testruns,
    )

    serialized = writer.serialize()
    reader = AggregationReader(serialized, now)

    tests = reader.get_test_aggregates(0, 2)
    for test in tests:
        test_dict = {
            "name": test.name,
            "test_id": test.test_id,# TODO
            "testsuite": test.testsuite,
            "flags": test.flags,
            "failure_rate": test.failure_rate,
            "flake_rate": test.flake_rate,
            "updated_at":test.updated_at,# TODO
            "avg_duration":test.avg_duration,
            "total_fail_count":test.total_fail_count,
            "total_flaky_fail_count":test.total_flaky_fail_count,
            "total_pass_count":test.total_pass_count,
            "total_skip_count":test.total_skip_count,
            "commits_where_fail":test.commits_where_fail,
            "last_duration":test.last_duration,# TODO
        }
        print(test_dict)