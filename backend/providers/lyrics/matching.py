from __future__ import annotations

from difflib import SequenceMatcher
from providers.lyrics.normalization import normalize_text


def compute_similarity(expected: str, actual: str) -> float:
    """Calculates weighted similarity ratio between two strings using token coverage and SequenceMatcher."""
    norm_expected = normalize_text(expected)
    norm_actual = normalize_text(actual)
    if not norm_expected or not norm_actual:
        return 0.0

    expected_tokens = set(norm_expected.split())
    actual_tokens = set(norm_actual.split())
    if not expected_tokens:
        return 0.0

    coverage = len(expected_tokens & actual_tokens) / len(expected_tokens)
    sequence = SequenceMatcher(None, norm_expected, norm_actual).ratio()
    return 0.65 * coverage + 0.35 * sequence

