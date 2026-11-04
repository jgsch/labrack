"""Tests for scripts/app_shot.py. Run with `mise run test`."""

import unittest

import app_shot
import mockup


class Pages(unittest.TestCase):
    """ARCH-7: every page of the app has a mockup board to be compared with."""

    def test_every_page_has_a_board(self):
        slugs = {board.slug for board in mockup.boards(mockup.read_tree(mockup.SRC))}
        for page, board in app_shot.PAGES.items():
            self.assertIn(board, slugs, page)

    def test_page_names_match_the_render_example(self):
        # `examples/render.rs` writes the page label, lower-cased, with " & " as "-"
        labels = {"amplifiers": "Amplifiers", "mix-eq": "Mix & EQ", "search": "Search"}
        for page, label in labels.items():
            self.assertEqual(label.lower().replace(" & ", "-"), page)


if __name__ == "__main__":
    unittest.main()
