"""Tests for selecting the previous published release for GitHub CI."""
import unittest

from semver_baseline import select_baseline


class BaselineTests(unittest.TestCase):
    def test_main_and_retried_tag(self):
        releases = [
            {"vers": v, "yanked": False}
            for v in ["0.3.1", "0.4.0", "0.2.0", "0.3.0"]
        ]
        self.assertEqual(select_baseline(releases, "0.3.1", tagged=False), "0.3.1")
        self.assertEqual(select_baseline(releases, "0.3.1", tagged=True), "0.3.0")
        self.assertEqual(select_baseline(releases, "0.3.2", tagged=True), "0.3.1")

    def test_semver_precedence_and_yanks(self):
        releases = [
            {"vers": v, "yanked": False}
            for v in ["1.10.0-rc.2", "1.9.9", "1.10.0-rc.10", "1.10.0+build.1"]
        ]
        releases.append({"vers": "1.10.0-rc.11", "yanked": True})
        self.assertEqual(select_baseline(releases, "1.10.0+build.2", tagged=True),
                         "1.10.0-rc.10")
        self.assertEqual(select_baseline(releases, "1.10.0", tagged=False),
                         "1.10.0+build.1")
        self.assertEqual(select_baseline(releases, "1.10.0-rc.10", tagged=True),
                         "1.10.0-rc.2")

    def test_no_eligible_release(self):
        with self.assertRaisesRegex(ValueError, "no eligible"):
            select_baseline([{"vers": "1.0.0", "yanked": False}], "1.0.0", tagged=True)
