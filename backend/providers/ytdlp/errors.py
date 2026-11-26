from __future__ import annotations

from exceptions import (
    BotCheckError,
    GeoBlockedError,
    NetworkError,
    ProviderError,
    RateLimitError,
    ResolverOutdatedError,
    StreamUnavailableError,
    TrackNotFoundError,
)
from exceptions import JsRuntimeError


def _unwrap_cause(exc: Exception) -> Exception:
    exc_info = getattr(exc, "exc_info", None)
    if isinstance(exc_info, tuple) and len(exc_info) > 1 and isinstance(exc_info[1], Exception):
        return exc_info[1]
    if isinstance(exc.__cause__, Exception):
        return exc.__cause__
    return exc


def _classify_message(msg: str, expected: bool) -> ProviderError:
    low = msg.lower()

    if any(tok in low for tok in ("http error 429", "too many requests", "has been rate-limited", "try again later")):
        return RateLimitError("Rate limited by YouTube. Please wait a bit before retrying.")

    if any(
        tok in low
        for tok in (
            "no supported javascript runtime",
            "js challenge",
            "challenge solver",
            "quickjs",
            "ejs",
            "nsig",
            "n challenge",
            "signature extraction",
        )
    ):
        if "no supported javascript runtime" in low:
            return JsRuntimeError("Missing JavaScript runtime for YouTube extraction.")
        return ResolverOutdatedError("YouTube challenge solver failed. Update yt-dlp with :update-ytdlp.")

    if any(
        tok in low
        for tok in (
            "sign in to confirm",
            "confirm you're not a bot",
            "confirm your age",
            "age-restricted",
            "age_verification_required",
            "age_check_required",
            "captcha challenge",
            "login required",
            "use --cookies",
        )
    ):
        return BotCheckError("YouTube requested sign-in or bot verification.")

    if any(
        tok in low
        for tok in (
            "requested format is not available",
            "no video formats found",
            "no playable audio",
            "unable to extract",
        )
    ):
        return StreamUnavailableError("No playable audio format is available for this track.")

    if any(
        tok in low
        for tok in (
            "video unavailable",
            "this video is unavailable",
            "private video",
            "video has been removed",
            "account associated with this video has been terminated",
            "premieres in",
            "members-only",
        )
    ):
        return TrackNotFoundError("Track is unavailable on YouTube.")

    if any(
        tok in low
        for tok in (
            "timed out",
            "timeout",
            "name resolution",
            "nodename nor servname",
            "connection refused",
            "connection reset",
            "temporary failure in name resolution",
            "getaddrinfo failed",
            "network is unreachable",
            "ssl",
            "urlopen error",
        )
    ):
        return NetworkError(f"Network error during stream resolution: {msg}")

    if expected:
        return TrackNotFoundError(msg or "Track is unavailable on YouTube.")

    return ProviderError(msg or "Stream resolution failed.")


def classify_ytdlp_error(exc: Exception) -> ProviderError:
    """Maps yt-dlp exceptions to structured ANSImuz provider errors."""
    if isinstance(exc, ProviderError):
        return exc

    cause = _unwrap_cause(exc)
    cause_name = type(cause).__name__
    outer_name = type(exc).__name__

    if "GeoRestrictedError" in (outer_name, cause_name):
        return GeoBlockedError("This track is not available in your region.")

    if "UnavailableVideoError" in (outer_name, cause_name):
        return TrackNotFoundError("Track is unavailable on YouTube.")

    if "JsChallengeProviderError" in (outer_name, cause_name):
        return JsRuntimeError(str(cause) or "JavaScript challenge solver failed.")

    if isinstance(cause, (ConnectionError, TimeoutError, OSError)):
        return NetworkError(f"Network error during stream resolution: {cause}")

    cause_inner = getattr(cause, "cause", None)
    if cause_inner is not None and getattr(cause_inner, "status", None) == 429:
        return RateLimitError("Rate limited by YouTube. Please wait a bit before retrying.")

    msg = str(cause) if str(cause) else str(exc)
    expected = bool(getattr(cause, "expected", False) or getattr(exc, "expected", False))
    return _classify_message(msg, expected)