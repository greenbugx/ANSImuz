%global commit 6143266e86a28c054c8d219b291f4b309132558d

# Keep distro LTO flags out of the aws-lc-sys C build (breaks linking with rust-lld).
%global _lto_cflags %{nil}

Name:           ansimuz
Version:        0.1.2
Release:        1%{?dist}
Summary:        A lightweight fast TUI to stream your favourite music with pure 8-bit ANSI vibes

License:        MIT
URL:            https://github.com/greenbugx/ANSImuz
Source0:        %{url}/archive/%{commit}/%{name}-%{commit}.tar.gz

BuildRequires:  cargo
BuildRequires:  cmake
BuildRequires:  gcc
BuildRequires:  make
BuildRequires:  python3-devel

Requires:       mpv
Requires:       yt-dlp
Requires:       python3
Requires:       python3-pillow
Requires:       python3-requests
Requires:       python3-ytmusicapi
Requires:       python3dist(spotifyscraper)

%description
ANSImuz is a lightweight terminal music player that streams music
from YouTube Music and Spotify with retro 8-bit ANSI aesthetics.

%prep
%autosetup -n ANSImuz-%{commit}

# Fetch Rust dependencies using the committed Cargo.lock.
# Needs "Enable internet access during builds" in the COPR project settings.
cargo fetch --locked

%build
export PYO3_PYTHON=%{__python3}
export ANSIMUZ_DATA_DIR=%{_datadir}/ansimuz

cargo build --frozen --release

%install
install -Dpm0755 target/release/ansimuz \
    %{buildroot}%{_bindir}/ansimuz

install -d %{buildroot}%{_datadir}/ansimuz

cp -a backend assets \
    %{buildroot}%{_datadir}/ansimuz/

%files
%license LICENSE
%doc README.md
%{_bindir}/ansimuz
%{_datadir}/ansimuz/

%changelog
* Sun Nov 01 2026 Greenbugx <jesuschetia.gg@gmail.com> - 0.1.2-1
- Release ANSImuz v0.1.2
