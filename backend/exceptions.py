from __future__ import annotations


class ProviderError(Exception):
    """Base exception for all music provider failures."""


class NetworkError(ProviderError):
    """Network connection, DNS, or socket timeout failure."""


class RateLimitError(ProviderError):
    """Provider API rate limit or quota exceeded (HTTP 429)."""


class AuthenticationError(ProviderError):
    """Authentication or authorization failure (HTTP 401/403)."""


class TrackNotFoundError(ProviderError, LookupError):
    """No matching track could be found on the target provider."""


class GeoBlockedError(ProviderError):
    """Track is unavailable in the user's region."""


class BotCheckError(AuthenticationError):
    """YouTube requested sign-in, age confirmation, or a captcha check."""


class StreamUnavailableError(ProviderError):
    """Extractor returned no playable audio stream for the track."""


class ResolverOutdatedError(ProviderError):
    """Stream challenge solver failed and yt-dlp likely needs an update."""


class JsRuntimeError(ResolverOutdatedError):
    """JavaScript challenge runtime is missing or failed to execute."""


def classify_error(context: str, exc: Exception) -> ProviderError:
    """Classifies an unexpected exception into a structured ProviderError subclass."""
    if isinstance(exc, ProviderError):
        return exc

    if type(exc).__module__.startswith("yt_dlp"):
        from providers.ytdlp.errors import classify_ytdlp_error

        return classify_ytdlp_error(exc)

    msg = str(exc).lower()

    # Check status code from response object if present
    status_code: int | None = None
    response = getattr(exc, "response", None)
    if response is not None:
        status_code = getattr(response, "status_code", None)

    # Rate limiting (429) 
    if (
        status_code == 429
        or "429" in msg
        or "rate limit" in msg
        or "too many requests" in msg
    ):
        return RateLimitError(f"{context}: rate limit exceeded")

    # Authentication / Authorization (401 / 403)
    if (
        status_code in (401, 403)
        or "401" in msg
        or "403" in msg
        or "unauthorized" in msg
        or "forbidden" in msg
        or "login required" in msg
    ):
        return AuthenticationError(f"{context}: authentication failed")

    # Network connection  
    # DNS  
    # timeout failures
    is_network_exc = False
    try:
        import requests

        if isinstance(
            exc,
            (
                requests.exceptions.ConnectionError,
                requests.exceptions.Timeout,
                requests.exceptions.ChunkedEncodingError,
            ),
        ):
            is_network_exc = True
    except ImportError:
        pass

    network_indicators = (
        "connection refused",
        "connection reset",
        "connection error",
        "network is unreachable",
        "name resolution",
        "nodename nor servname",
        "failed to establish a new connection",
        "max retries exceeded",
        "timed out",
        "timeout",
        "read timed out",
        "connect timeout",
    )

    if (
        is_network_exc
        or isinstance(exc, (ConnectionError, TimeoutError, OSError))
        or any(indicator in msg for indicator in network_indicators)
    ):
        return NetworkError(f"{context}: network connection error ({exc})")

    # Fallback to general ProviderError
    return ProviderError(f"{context}: {exc}")
