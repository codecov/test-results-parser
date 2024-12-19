import pytest
import base64
import zlib
import json
import msgpack
from test_results_parser import parse_raw_upload

class TestParsers:
    @pytest.mark.parametrize(
        "filename,expected",
        [
            (
                "./tests/junit.xml",
                {
                    "framework": "Pytest",
                    "testruns": [
                        {
                            "name": "test_junit[junit.xml--True]",
                            "classname": "tests.test_parsers.TestParsers",
                            "duration": 0.001,
                            "outcome": "failure",
                            "testsuite": "pytest",
                            "failure_message": """self = <test_parsers.TestParsers object at 0x102182d10>, filename = 'junit.xml', expected = '', check = True

    @pytest.mark.parametrize(
        \"filename,expected,check\",
        [(\"junit.xml\", \"\", True), (\"jest-junit.xml\", \"\", False)],
    )
    def test_junit(self, filename, expected, check):
        with open(filename) as f:
            junit_string = f.read()
            res = parse_junit_xml(junit_string)
            print(res)
            if check:
>               assert res == expected
E               AssertionError: assert [{'duration': '0.010', 'name': 'tests.test_parsers.TestParsers.test_junit[junit.xml-]', 'outcome': 'failure'}, {'duration': '0.063', 'name': 'tests.test_parsers.TestParsers.test_junit[jest-junit.xml-]', 'outcome': 'pass'}] == ''

tests/test_parsers.py:16: AssertionError""",
                            "filename": None,
                            "build_url": None,
                            "computed_name": "tests.test_parsers.TestParsers::test_junit[junit.xml--True]",
                        },
                        {
                            "name": "test_junit[jest-junit.xml--False]",
                            "classname": "tests.test_parsers.TestParsers",
                            "duration": 0.064,
                            "outcome": "pass",
                            "testsuite": "pytest",
                            "failure_message": None,
                            "filename": None,
                            "build_url": None,
                            "computed_name": "tests.test_parsers.TestParsers::test_junit[jest-junit.xml--False]",
                        },
                    ],
                },
            ),
            (
                "./tests/junit-no-testcase-timestamp.xml",
                {
                    "framework": "Pytest",
                    "testruns": [
                        {
                            "name": "test_junit[junit.xml--True]",
                            "classname": "tests.test_parsers.TestParsers",
                            "duration": 0.186,
                            "outcome": "failure",
                            "testsuite": "pytest",
                            "failure_message": "aaaaaaa",
                            "filename": None,
                            "build_url": None,
                            "computed_name": "tests.test_parsers.TestParsers::test_junit[junit.xml--True]",
                        },
                        {
                            "name": "test_junit[jest-junit.xml--False]",
                            "classname": "tests.test_parsers.TestParsers",
                            "duration": 0.186,
                            "outcome": "pass",
                            "testsuite": "pytest",
                            "failure_message": None,
                            "filename": None,
                            "build_url": None,
                            "computed_name": "tests.test_parsers.TestParsers::test_junit[jest-junit.xml--False]",
                        },
                    ],
                },
            ),
            (
                "./tests/junit-nested-testsuite.xml",
                {
                    "framework": "Pytest",
                    "testruns": [
                        {
                            "name": "test_junit[junit.xml--True]",
                            "classname": "tests.test_parsers.TestParsers",
                            "duration": 0.186,
                            "outcome": "failure",
                            "testsuite": "nested_testsuite",
                            "failure_message": "aaaaaaa",
                            "filename": None,
                            "build_url": None,
                            "computed_name": None,
                        },
                        {
                            "name": "test_junit[jest-junit.xml--False]",
                            "classname": "tests.test_parsers.TestParsers",
                            "duration": 0.186,
                            "outcome": "pass",
                            "testsuite": "pytest",
                            "failure_message": None,
                            "filename": None,
                            "build_url": None,
                            "computed_name": "tests.test_parsers.TestParsers::test_junit[jest-junit.xml--False]",
                        },
                    ],
                },
            ),
            (
                "./tests/jest-junit.xml",
                {
                    "framework": "Jest",
                    "testruns": [
                        {
                            "name": "Title when rendered renders pull title",
                            "classname": "Title when rendered renders pull title",
                            "duration": 0.036,
                            "outcome": "pass",
                            "testsuite": "Title",
                            "failure_message": None,
                            "filename": None,
                            "build_url": None,
                            "computed_name": "Title when rendered renders pull title",
                        },
                        {
                            "name": "Title when rendered renders pull author",
                            "classname": "Title when rendered renders pull author",
                            "duration": 0.005,
                            "outcome": "pass",
                            "testsuite": "Title",
                            "failure_message": None,
                            "filename": None,
                            "build_url": None,
                            "computed_name": "Title when rendered renders pull author",
                        },
                        {
                            "name": "Title when rendered renders pull updatestamp",
                            "classname": "Title when rendered renders pull updatestamp",
                            "duration": 0.002,
                            "outcome": "pass",
                            "testsuite": "Title",
                            "failure_message": None,
                            "filename": None,
                            "build_url": None,
                            "computed_name": "Title when rendered renders pull updatestamp",
                        },
                        {
                            "name": "Title when rendered for first pull request renders pull title",
                            "classname": "Title when rendered for first pull request renders pull title",
                            "duration": 0.006,
                            "outcome": "pass",
                            "testsuite": "Title",
                            "failure_message": None,
                            "filename": None,
                            "build_url": None,
                            "computed_name": "Title when rendered for first pull request renders pull title",
                        },
                    ],
                },
            ),
            (
                "./tests/vitest-junit.xml",
                {
                    "framework": "Vitest",
                    "testruns": [
                        {
                            "name": "first test file &gt; 2 + 2 should equal 4",
                            "classname": "__tests__/test-file-1.test.ts",
                            "duration": 0.01,
                            "outcome": "failure",
                            "testsuite": "__tests__/test-file-1.test.ts",
                            "failure_message": """AssertionError: expected 5 to be 4 // Object.is equality
 ❯ __tests__/test-file-1.test.ts:20:28""",
                            "filename": None,
                            "build_url": None,
                            "computed_name": "__tests__/test-file-1.test.ts > first test file > 2 + 2 should equal 4",
                        },
                        {
                            "name": "first test file &gt; 4 - 2 should equal 2",
                            "classname": "__tests__/test-file-1.test.ts",
                            "duration": 0.0,
                            "outcome": "pass",
                            "testsuite": "__tests__/test-file-1.test.ts",
                            "failure_message": None,
                            "filename": None,
                            "build_url": None,
                            "computed_name": "__tests__/test-file-1.test.ts > first test file > 4 - 2 should equal 2",
                        },
                    ],
                },
            ),
            (
                "./tests/empty_failure.junit.xml",
                {
                    "framework": None,
                    "testruns": [
                        {
                            "name": "test.test works",
                            "classname": "test.test",
                            "duration": 0.234,
                            "outcome": "pass",
                            "testsuite": "test",
                            "failure_message": None,
                            "filename": "./test.rb",
                            "build_url": None,
                            "computed_name": None,
                        },
                        {
                            "name": "test.test fails",
                            "classname": "test.test",
                            "duration": 1.0,
                            "outcome": "failure",
                            "testsuite": "test",
                            "failure_message": "TestError",
                            "filename": "./test.rb",
                            "build_url": None,
                            "computed_name": None,
                        },
                    ],
                },
            ),
            (
                "./tests/phpunit.junit.xml",
                {
                    "framework": "PHPUnit",
                    "testruns": [
                        {
                            "name": "test1",
                            "classname": "class.className",
                            "duration": 0.1,
                            "outcome": "pass",
                            "testsuite": "Thing",
                            "failure_message": None,
                            "filename": "/file1.php",
                            "build_url": None,
                            "computed_name": "class.className::test1",
                        },
                        {
                            "name": "test2",
                            "classname": "",
                            "duration": 0.1,
                            "outcome": "pass",
                            "testsuite": "Thing",
                            "failure_message": None,
                            "filename": "/file1.php",
                            "build_url": None,
                            "computed_name": "::test2",
                        },
                    ],
                },
            ),
            (
                "./tests/ctest.xml",
                {
                    "framework": None,
                    "testruns": [
                        {
                            "name": "a_unit_test",
                            "classname": "a_unit_test",
                            "duration": 33.4734,
                            "outcome": "failure",
                            "testsuite": "Linux-c++",
                            "failure_message": "Failed",
                            "filename": None,
                            "build_url": None,
                            "computed_name": None,
                        }
                    ],
                },
            ),
            (
                "./tests/no-testsuite-name.xml",
                {
                    "framework": None,
                    "testruns": [
                        {
                            "name": "a_unit_test",
                            "classname": "a_unit_test",
                            "duration": 33.4734,
                            "outcome": "failure",
                            "testsuite": "",
                            "failure_message": "Failed",
                            "filename": None,
                            "build_url": None,
                            "computed_name": None,
                        }
                    ],
                },
            ),
            ("./tests/testsuites.xml", {"framework": None, "testruns": []}),
        ],
    )
    def test_junit(self, filename, expected):
        with open(filename, "b+r") as f:
            file_bytes = f.read()
            thing = {
                "network": [
                    "a/b/c.py",
                ],
                "test_results_files": [
                    {
                        "filename": filename,
                        "format": "base64+compressed",
                        "data": base64.b64encode(zlib.compress(file_bytes)).decode(
                            "utf-8"
                        ),
                    }
                ]
            }
            json_bytes = json.dumps(thing).encode("utf-8")
            msgpack_bytes, readable_files_bytes = parse_raw_upload(json_bytes)



            res_list = msgpack.unpackb(
                bytes(msgpack_bytes)
            )

            readable_files = bytes(readable_files_bytes)

            assert readable_files == f"""# path={filename}\n{file_bytes.decode()}\n<<<<<< EOF\n""".encode()
            

            assert res_list[0]["framework"] == expected["framework"]
            assert res_list[0]["testruns"] == expected["testruns"]
            assert len(res_list[0]["testruns"]) == len(expected["testruns"])
            for restest, extest in zip(res_list[0]["testruns"], expected["testruns"]):
                print(
                    restest["classname"],
                    restest["duration"],
                    restest["filename"],
                    restest["name"],
                    restest["outcome"],
                    restest["testsuite"],
                    extest["failure_message"],
                    extest["filename"],
                    extest["computed_name"],
                )
                assert restest == extest
