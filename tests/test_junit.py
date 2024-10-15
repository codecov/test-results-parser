import pytest
from test_results_parser import (
    Framework,
    Outcome,
    ParsingInfo,
    Testrun,
    parse_junit_xml,
)


class TestParsers:
    @pytest.mark.parametrize(
        "filename,expected",
        [
            (
                "./tests/junit.xml",
                ParsingInfo(
                    Framework.Pytest,
                    [
                        Testrun(
                            "test_junit[junit.xml--True]",
                            "tests.test_parsers.TestParsers",
                            0.001,
                            Outcome.Failure,
                            "pytest",
                            """self = &lt;test_parsers.TestParsers object at 0x102182d10&gt;, filename = 'junit.xml', expected = '', check = True

    @pytest.mark.parametrize(
        "filename,expected,check",
        [("junit.xml", "", True), ("jest-junit.xml", "", False)],
    )
    def test_junit(self, filename, expected, check):
        with open(filename) as f:
            junit_string = f.read()
            res = parse_junit_xml(junit_string)
            print(res)
            if check:
&gt;               assert res == expected
E               AssertionError: assert [{'duration': '0.010', 'name': 'tests.test_parsers.TestParsers.test_junit[junit.xml-]', 'outcome': 'failure'}, {'duration': '0.063', 'name': 'tests.test_parsers.TestParsers.test_junit[jest-junit.xml-]', 'outcome': 'pass'}] == ''

tests/test_parsers.py:16: AssertionError""",
                            None,
                        ),
                        Testrun(
                            "test_junit[jest-junit.xml--False]",
                            "tests.test_parsers.TestParsers",
                            0.064,
                            Outcome.Pass,
                            "pytest",
                            None,
                            None,
                        ),
                    ],
                ),
            ),
            (
                "./tests/junit-no-testcase-timestamp.xml",
                ParsingInfo(
                    Framework.Pytest,
                    [
                        Testrun(
                            "test_junit[junit.xml--True]",
                            "tests.test_parsers.TestParsers",
                            0.186,
                            Outcome.Failure,
                            "pytest",
                            """aaaaaaa""",
                            None,
                        ),
                        Testrun(
                            "test_junit[jest-junit.xml--False]",
                            "tests.test_parsers.TestParsers",
                            0.186,
                            Outcome.Pass,
                            "pytest",
                            None,
                            None,
                        ),
                    ],
                ),
            ),
            (
                "./tests/junit-nested-testsuite.xml",
                ParsingInfo(
                    Framework.Pytest,
                    [
                        Testrun(
                            "test_junit[junit.xml--True]",
                            "tests.test_parsers.TestParsers",
                            0.186,
                            Outcome.Failure,
                            "nested_testsuite",
                            """aaaaaaa""",
                            None,
                        ),
                        Testrun(
                            "test_junit[jest-junit.xml--False]",
                            "tests.test_parsers.TestParsers",
                            0.186,
                            Outcome.Pass,
                            "pytest",
                            None,
                            None,
                        ),
                    ],
                ),
            ),
            (
                "./tests/jest-junit.xml",
                ParsingInfo(
                    Framework.Jest,
                    [
                        Testrun(
                            "Title when rendered renders pull title",
                            "Title when rendered renders pull title",
                            0.036,
                            Outcome.Pass,
                            "Title",
                            None,
                            None,
                        ),
                        Testrun(
                            "Title when rendered renders pull author",
                            "Title when rendered renders pull author",
                            0.005,
                            Outcome.Pass,
                            "Title",
                            None,
                            None,
                        ),
                        Testrun(
                            "Title when rendered renders pull updatestamp",
                            "Title when rendered renders pull updatestamp",
                            0.002,
                            Outcome.Pass,
                            "Title",
                            None,
                            None,
                        ),
                        Testrun(
                            "Title when rendered for first pull request renders pull title",
                            "Title when rendered for first pull request renders pull title",
                            0.006,
                            Outcome.Pass,
                            "Title",
                            None,
                            None,
                        ),
                    ],
                ),
            ),
            (
                "./tests/vitest-junit.xml",
                ParsingInfo(
                    Framework.Vitest,
                    [
                        Testrun(
                            "first test file &gt; 2 + 2 should equal 4",
                            "__tests__/test-file-1.test.ts",
                            0.01,
                            Outcome.Failure,
                            "__tests__/test-file-1.test.ts",
                            """AssertionError: expected 5 to be 4 // Object.is equality
 ❯ __tests__/test-file-1.test.ts:20:28""",
                            None,
                        ),
                        Testrun(
                            "first test file &gt; 4 - 2 should equal 2",
                            "__tests__/test-file-1.test.ts",
                            0,
                            Outcome.Pass,
                            "__tests__/test-file-1.test.ts",
                            None,
                            None,
                        ),
                    ],
                ),
            ),
            (
                "./tests/empty_failure.junit.xml",
                ParsingInfo(
                    None,
                    [
                        Testrun(
                            "test.test works",
                            "test.test",
                            0.234,
                            Outcome.Pass,
                            "test",
                            None,
                            "test.rb",
                        ),
                        Testrun(
                            "test.test fails",
                            "test.test",
                            1,
                            Outcome.Failure,
                            "test",
                            "TestError",
                            None,
                        ),
                    ],
                ),
            ),
            (
                "./tests/phpunit.junit.xml",
                ParsingInfo(
                    Framework.PHPUnit,
                    [
                        Testrun(
                            "test1",
                            "class.className",
                            0.1,
                            Outcome.Pass,
                            "Thing",
                            None,
                            "/file1.php",
                        ),
                        Testrun(
                            "test2",
                            "",
                            0.1,
                            Outcome.Pass,
                            "Thing",
                            None,
                            "/file1.php",
                        ),
                    ],
                ),
            ),
            (
                "./tests/no-testsuites.xml",
                ParsingInfo(
                    None,
                    [
                        Testrun(
                            "test_name",
                            "test_classname",
                            0.1,
                            Outcome.Pass,
                            "testsuite_name",
                            None,
                            None,
                        ),
                    ],
                ),
            ),
        ],
    )
    def test_junit(self, filename, expected):
        with open(filename, "b+r") as f:
            res = parse_junit_xml(f.read())
            assert res.framework == expected.framework
            assert len(res.testruns) == len(expected.testruns)
            for restest, extest in zip(res.testruns, expected.testruns):
                print(
                    restest.classname,
                    restest.duration,
                    restest.filename,
                    restest.name,
                    restest.outcome,
                    restest.testsuite,
                )
                print(
                    extest.classname,
                    extest.duration,
                    extest.filename,
                    extest.name,
                    extest.outcome,
                    extest.testsuite,
                )
                assert restest == extest
