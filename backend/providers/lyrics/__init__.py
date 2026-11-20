from __future__ import annotations

import logging
import threading
import time
from dataclasses import dataclass
from typing import Any, Callable

import requests

from exceptions import (
    AuthenticationError,
    NetworkError,
    ProviderError,
    RateLimitError,
)
from providers.lyrics.base import (
    DEFAULT_ATTRIBUTION,
    DEFAULT_TIMEOUT,
    DEFAULT_USER_AGENT,
    LyricsProvider,
    build_lyrics_response,
    clean_title_for_search,
    format_timestamp,
    parse_lrc,
    parse_lyrics_plus,
    parse_lyricsfile,
    parse_qrc,
    parse_timestamp,
    parse_ttml,
)
from providers.lyrics.better_lyrics import (
    DEFAULT_BETTER_LYRICS_ATTRIBUTION,
    BetterLyricsProvider,
)
from providers.lyrics.genius import (
    DEFAULT_GENIUS_ATTRIBUTION,
    GeniusProvider,
)
from providers.lyrics.lrclib import (
    DEFAULT_LRCLIB_ATTRIBUTION,
    LrclibProvider,
)
from providers.lyrics.lyrics_plus import (
    DEFAULT_LYRICS_PLUS_ATTRIBUTION,
    LyricsPlusProvider,
)
from providers.lyrics.lrc_red import LrcRedProvider
from providers.lyrics.megalobiz import (
    DEFAULT_MEGALOBIZ_ATTRIBUTION,
    MegalobizProvider,
)

logger = logging.getLogger(__name__)

class LyricsProviderRegistry:
    """
    Registry managing lazy initialization and lookup of lyrics providers.
    Eliminates duplicated global provider state while preserving lazy instantiation
    and test mock compatibility.
    """

    def __init__(self) -> None:
        self._factories: dict[str, Callable[[], Any]] = {
            "lrc.red": LrcRedProvider,
            "LyricsPlus": LyricsPlusProvider,
            "LRCLIB": LrclibProvider,
            "Megalobiz": MegalobizProvider,
            "BetterLyrics": BetterLyricsProvider,
            "Genius": GeniusProvider,
        }
        self._instances: dict[str, Any] = {}
        self._lock = threading.Lock()

    def register(self, name: str, factory_or_instance: Any) -> None:
        """Registers a provider factory or pre-constructed instance."""
        with self._lock:
            if callable(factory_or_instance):
                self._factories[name] = factory_or_instance
                self._instances.pop(name, None)
            else:
                self._instances[name] = factory_or_instance

    def get(self, name: str) -> Any:
        """Retrieves or lazily constructs the requested provider."""
        with self._lock:
            if name in self._instances:
                return self._instances[name]
            factory = self._factories.get(name)
            if factory is None:
                raise KeyError(f"Unknown lyrics provider: {name}")
            instance = factory()
            self._instances[name] = instance
            return instance

    def reset(self) -> None:
        """Clears all cached instances (useful for test isolation)."""
        with self._lock:
            self._instances.clear()

    @property
    def registered_names(self) -> list[str]:
        return list(self._factories.keys())


_GLOBAL_REGISTRY = LyricsProviderRegistry()


def get_provider_registry() -> LyricsProviderRegistry:
    return _GLOBAL_REGISTRY


def get_better_lyrics_provider() -> BetterLyricsProvider:
    return _GLOBAL_REGISTRY.get("BetterLyrics")


def get_genius_provider() -> GeniusProvider:
    return _GLOBAL_REGISTRY.get("Genius")


def get_lyrics_plus_provider() -> LyricsPlusProvider:
    return _GLOBAL_REGISTRY.get("LyricsPlus")


def get_lrclib_provider() -> LrclibProvider:
    return _GLOBAL_REGISTRY.get("LRCLIB")


def get_megalobiz_provider() -> MegalobizProvider:
    return _GLOBAL_REGISTRY.get("Megalobiz")


def get_lrc_red_provider() -> LrcRedProvider:
    return _GLOBAL_REGISTRY.get("lrc.red")


# Backward compatibility alias
get_default_provider = get_lrc_red_provider

DEFAULT_TOTAL_DEADLINE_SECONDS = 18.0


@dataclass
class ProviderResult:
    """Internal diagnostic record of a lyrics provider execution."""

    lyrics: dict[str, Any] | None = None
    error: str | None = None
    status_code: int | None = None
    provider_name: str = ""
    latency_ms: float = 0.0


class ProviderHealthTracker:
    """
    Tracks failure rates and short-lived backoff state for lyrics providers.
    Prevents repeated expensive network timeouts on providers experiencing
    persistent outages or active HTTP 429 rate limits.
    """

    def __init__(
        self,
        base_backoff_sec: float = 30.0,
        rate_limit_backoff_sec: float = 60.0,
        max_backoff_sec: float = 180.0,
        failure_threshold: int = 2,
    ) -> None:
        self.base_backoff_sec = base_backoff_sec
        self.rate_limit_backoff_sec = rate_limit_backoff_sec
        self.max_backoff_sec = max_backoff_sec
        self.failure_threshold = failure_threshold
        self._consecutive_failures: dict[str, int] = {}
        self._backoff_until: dict[str, float] = {}
        self._lock = threading.Lock()

    def is_available(self, provider_name: str) -> bool:
        """Returns True if the provider is healthy or its backoff period has expired."""
        with self._lock:
            until = self._backoff_until.get(provider_name, 0.0)
            return time.monotonic() >= until

    def get_backoff_remaining(self, provider_name: str) -> float:
        """Returns seconds remaining in backoff, or 0.0 if healthy."""
        with self._lock:
            until = self._backoff_until.get(provider_name, 0.0)
            return max(0.0, until - time.monotonic())

    def record_success(self, provider_name: str) -> None:
        """Records a successful lyrics retrieval, clearing any failure counters."""
        with self._lock:
            self._consecutive_failures.pop(provider_name, None)
            self._backoff_until.pop(provider_name, None)

    def record_result(self, result: ProviderResult) -> None:
        """Updates health status based on the provider result."""
        name = result.provider_name
        if result.lyrics:
            self.record_success(name)
            return

        # Normal not-found (404 / empty lines) is not an infrastructure failure
        if result.status_code == 404 or (
            result.error == "not_found" and result.status_code in (None, 200, 404)
        ):
            with self._lock:
                self._consecutive_failures.pop(name, None)
            return

        with self._lock:
            count = self._consecutive_failures.get(name, 0) + 1
            self._consecutive_failures[name] = count

            # HTTP 429: immediate backoff
            if result.status_code == 429 or "rate limit" in (result.error or "").lower():
                backoff = min(
                    self.rate_limit_backoff_sec * (2 ** (count - 1)),
                    self.max_backoff_sec,
                )
                self._backoff_until[name] = time.monotonic() + backoff
                logger.warning(
                    "Lyrics provider '%s' rate-limited (HTTP 429); backing off for %.1fs",
                    name,
                    backoff,
                )
                return

            # Consecutive network failures, timeouts, or 5xx server errors
            is_infra_error = (
                result.error in ("timeout", "connection_error")
                or (result.status_code is not None and result.status_code >= 500)
                or "crash" in (result.error or "").lower()
            )
            if is_infra_error and count >= self.failure_threshold:
                backoff = min(
                    self.base_backoff_sec * (2 ** (count - self.failure_threshold)),
                    self.max_backoff_sec,
                )
                self._backoff_until[name] = time.monotonic() + backoff
                logger.warning(
                    "Lyrics provider '%s' persistent failure (%s, count=%d); backing off for %.1fs",
                    name,
                    result.error,
                    count,
                    backoff,
                )

    def reset(self) -> None:
        """Resets all provider health states and backoff windows."""
        with self._lock:
            self._consecutive_failures.clear()
            self._backoff_until.clear()


_GLOBAL_HEALTH = ProviderHealthTracker()


def get_provider_health() -> ProviderHealthTracker:
    return _GLOBAL_HEALTH


def reset_provider_health() -> None:
    _GLOBAL_HEALTH.reset()


_MINIMUM_PROVIDER_TIMEOUT = 1.0  # Never shrink per-request timeout below this floor


def _invoke_provider(
    provider_name: str,
    provider_getter: Callable[[], Any],
    track: dict[str, Any],
    deadline: float | None = None,
) -> ProviderResult:
    """Executes a single provider and returns a structured diagnostic ProviderResult.

    When *deadline* is supplied the provider's per-request timeout is capped to
    the remaining budget so that a slow provider cannot consume more wall-clock
    time than the caller's global deadline allows.  A floor of
    ``_MINIMUM_PROVIDER_TIMEOUT`` is applied so that the provider still gets a
    fair chance even when the budget is nearly exhausted.
    """
    t0 = time.monotonic()
    try:
        provider = provider_getter()

        # Temporarily cap the provider's per-request timeout to the remaining
        # deadline budget so that individual HTTP calls cannot outlast it.
        original_timeout: float | None = None
        if deadline is not None and hasattr(provider, "timeout") and isinstance(
            provider.timeout, (int, float)
        ):
            remaining = max(_MINIMUM_PROVIDER_TIMEOUT, deadline - t0)
            if remaining < provider.timeout:
                original_timeout = provider.timeout
                provider.timeout = remaining

        try:
            res = provider.get_lyrics(track)
        finally:
            # Always restore the original timeout so shared provider instances
            # don't leak a narrowed budget into subsequent requests.
            if original_timeout is not None:
                provider.timeout = original_timeout
        latency = (time.monotonic() - t0) * 1000.0
        if res and res.get("lines"):
            return ProviderResult(
                lyrics=res,
                error=None,
                status_code=200,
                provider_name=provider_name,
                latency_ms=latency,
            )
        return ProviderResult(
            lyrics=None,
            error="not_found",
            status_code=404,
            provider_name=provider_name,
            latency_ms=latency,
        )
    except RateLimitError as exc:
        latency = (time.monotonic() - t0) * 1000.0
        return ProviderResult(
            lyrics=None,
            error=str(exc),
            status_code=429,
            provider_name=provider_name,
            latency_ms=latency,
        )
    except AuthenticationError as exc:
        latency = (time.monotonic() - t0) * 1000.0
        return ProviderResult(
            lyrics=None,
            error=str(exc),
            status_code=401,
            provider_name=provider_name,
            latency_ms=latency,
        )
    except NetworkError as exc:
        latency = (time.monotonic() - t0) * 1000.0
        return ProviderResult(
            lyrics=None,
            error="connection_error",
            status_code=None,
            provider_name=provider_name,
            latency_ms=latency,
        )
    except requests.HTTPError as exc:
        latency = (time.monotonic() - t0) * 1000.0
        code = exc.response.status_code if exc.response is not None else None
        return ProviderResult(
            lyrics=None,
            error=str(exc),
            status_code=code,
            provider_name=provider_name,
            latency_ms=latency,
        )
    except (requests.Timeout, TimeoutError) as exc:
        latency = (time.monotonic() - t0) * 1000.0
        return ProviderResult(
            lyrics=None,
            error="timeout",
            status_code=None,
            provider_name=provider_name,
            latency_ms=latency,
        )
    except (requests.RequestException, OSError) as exc:
        latency = (time.monotonic() - t0) * 1000.0
        return ProviderResult(
            lyrics=None,
            error=str(exc),
            status_code=None,
            provider_name=provider_name,
            latency_ms=latency,
        )
    except Exception as exc:
        latency = (time.monotonic() - t0) * 1000.0
        msg = str(exc)
        code = None
        response = getattr(exc, "response", None)
        if response is not None:
            code = getattr(response, "status_code", None)
        if code is None and ("429" in msg.lower() or "rate limit" in msg.lower()):
            code = 429
        return ProviderResult(
            lyrics=None,
            error=msg,
            status_code=code,
            provider_name=provider_name,
            latency_ms=latency,
        )


def get_lyrics(
    track: dict[str, Any],
    is_cancelled: Any = None,
    deadline_seconds: float = DEFAULT_TOTAL_DEADLINE_SECONDS,
) -> dict[str, Any] | None:
    """
    Fetches normalized lyrics for a track dictionary using a tiered provider chain:
    1. lrc.red (LRCLIB ISRC provider with line/word-level sync)
    2. LyricsPlus (Aggregated Apple Music, QQ, Musixmatch syllable timings via mirror race)
    3. LRCLIB.net (Official LRCLIB database with Lyricsfile word sync & LRC line sync)
    4. Megalobiz (Megalobiz community synchronized LRC sheets via official REST API)
    5. BetterLyrics (Apple Music TTML syllable-timed & Portato QQ Music QRC fallback)
    6. Genius (Web scraper fallback providing unsynchronized plain lyrics with stanzas)

    Respects a global retrieval deadline and cancellation tokens. Employs short-lived
    health backoff for providers experiencing rate limits or persistent network outages.
    """
    def cancelled() -> bool:
        if is_cancelled is None:
            return False
        try:
            return bool(is_cancelled())
        except Exception:
            return False

    deadline = time.monotonic() + max(0.001, float(deadline_seconds))
    health = _GLOBAL_HEALTH

    provider_chain: list[tuple[str, Callable[[], Any]]] = [
        ("lrc.red", get_lrc_red_provider),
        ("LyricsPlus", get_lyrics_plus_provider),
        ("LRCLIB", get_lrclib_provider),
        ("Megalobiz", get_megalobiz_provider),
        ("BetterLyrics", get_better_lyrics_provider),
        ("Genius", get_genius_provider),
    ]

    for name, getter in provider_chain:
        if cancelled():
            logger.debug("Lyrics retrieval cancelled before provider '%s'", name)
            return None

        now = time.monotonic()
        if now >= deadline:
            logger.debug(
                "Lyrics retrieval deadline exceeded (%.1fs) before provider '%s'",
                deadline_seconds,
                name,
            )
            return None

        # Check circuit-breaker / health backoff
        if not health.is_available(name):
            rem = health.get_backoff_remaining(name)
            logger.debug(
                "Skipping provider '%s' due to active health backoff (%.1fs remaining)",
                name,
                rem,
            )
            continue

        result = _invoke_provider(name, getter, track, deadline=deadline)
        health.record_result(result)

        if result.lyrics:
            logger.debug(
                "Provider '%s' succeeded in %.1fms (sync=%s)",
                name,
                result.latency_ms,
                result.lyrics.get("sync_type"),
            )
            return result.lyrics

        logger.debug(
            "Provider '%s' failed in %.1fms (error=%s, status=%s)",
            name,
            result.latency_ms,
            result.error,
            result.status_code,
        )

    return None


__all__ = [
    "DEFAULT_ATTRIBUTION",
    "DEFAULT_BETTER_LYRICS_ATTRIBUTION",
    "DEFAULT_GENIUS_ATTRIBUTION",
    "DEFAULT_LRCLIB_ATTRIBUTION",
    "DEFAULT_LYRICS_PLUS_ATTRIBUTION",
    "DEFAULT_MEGALOBIZ_ATTRIBUTION",
    "DEFAULT_TIMEOUT",
    "DEFAULT_TOTAL_DEADLINE_SECONDS",
    "DEFAULT_USER_AGENT",
    "BetterLyricsProvider",
    "GeniusProvider",
    "LrclibProvider",
    "LyricsPlusProvider",
    "LyricsProvider",
    "LyricsProviderRegistry",
    "LrcRedProvider",
    "MegalobizProvider",
    "ProviderHealthTracker",
    "ProviderResult",
    "build_lyrics_response",
    "clean_title_for_search",
    "format_timestamp",
    "get_better_lyrics_provider",
    "get_default_provider",
    "get_genius_provider",
    "get_lrc_red_provider",
    "get_lrclib_provider",
    "get_lyrics",
    "get_lyrics_plus_provider",
    "get_megalobiz_provider",
    "get_provider_health",
    "get_provider_registry",
    "parse_lrc",
    "parse_lyrics_plus",
    "parse_lyricsfile",
    "parse_qrc",
    "parse_timestamp",
    "parse_ttml",
    "reset_provider_health",
]
