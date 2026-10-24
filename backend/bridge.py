from providers import yt_music

def search_music(query):
    """
    Main bridge function invoked directly by Rust via PyO3.
    """
    # for now pull directly from YT Music
    return yt_music.search(query)