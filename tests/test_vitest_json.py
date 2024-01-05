import pytest
from test_results_parser import parse_vitest_json, Testrun, Outcome


def test_vitest_json():
    expected = [
        Testrun(
            " first test file 2 + 2 should equal 4",
            0.009,
            Outcome.Failure,
            "/root-directory/__tests__/test-file-1.test.ts",
            "expected 5 to be 4 // Object.is equality",
        ),
        Testrun(
            " first test file 2 + 2 should equal 4",
            0.009,
            Outcome.Failure,
            "/root-directory/__tests__/test-file-1.test.ts",
            "expected 5 to be 4 // Object.is equality",
        ),
        Testrun(
            " first test file 2 + 2 should equal 4",
            0.009,
            Outcome.Failure,
            "/root-directory/__tests__/test-file-1.test.ts",
            "expected 5 to be 4 // Object.is equality",
        ),
        Testrun(
            " first test file 2 + 2 should equal 4",
            0.009,
            Outcome.Failure,
            "/root-directory/__tests__/test-file-1.test.ts",
            "expected 5 to be 4 // Object.is equality",
        ),
    ]

    with open("tests/vitest.json", "b+r") as f:
        testruns = parse_vitest_json(f.read())

        assert len(testruns) == len(expected)
        for restest, extest in zip(testruns, expected):
            assert restest == extest
