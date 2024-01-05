import pytest
from test_results_parser import parse_pytest_reportlog, Testrun, Outcome


def test_reportlog():
    expected = [
        Testrun(
            "TestParsers.test_junit[./tests/junit.xml-expected0]",
            0.011125802993774414,
            Outcome.Failure,
            "tests/test_junit.py",
            """AssertionError: assert (tests.test_parsers.TestParsers::test_junit[junit.xml--True], Failure, 0.001, pytest, self = &lt;test_parsers.TestPars...t_parsers.TestParsers.test_junit[jest-junit.xml-]', 'outcome': 'pass'}] == ''\\n\\ntests/test_parsers.py:16: AssertionErro) == (tests.test_parsers.TestParsers::test_junit[junit.xml--True], Failure, 0.001, pytest, self = &lt;test_parsers.TestPars..._parsers.TestParsers.test_junit[jest-junit.xml-]', 'outcome': 'pass'}] == ''\\n\\ntests/test_parsers.py:16: AssertionError)""",
        ),
        Testrun(
            "TestParsers.test_junit[./tests/jest-junit.xml-expected1]",
            0.0010750293731689453,
            Outcome.Pass,
            "tests/test_junit.py",
            "",
        ),
        Testrun(
            "TestParsers.test_junit[./tests/vitest-junit.xml-expected2]",
            0.0008599758148193359,
            Outcome.Pass,
            "tests/test_junit.py",
            "",
        ),
    ]

    with open("tests/log.jsonl", "b+r") as f:
        testruns = parse_pytest_reportlog(f.read())
        assert len(testruns) == len(expected)
        for restest, extest in zip(testruns, expected):
            assert restest == extest
