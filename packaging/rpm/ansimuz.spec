%global commit 0abb05156e8e81ae9a01ad6e3650202979db16b7
%global shortcommit %(c=%{commit}; echo ${c:0:7})
%global commitdate 20261031

# Keep distro LTO flags out of the aws-lc-sys C build (breaks linking with rust-lld).
%global _lto_cflags %{nil}

Name:           ansimuz
# Post-release snapshot of upstream v0.1.0 (caret sorts after 0.1.0, before 0.1.1)
Version:        0.1.0^%{commitdate}git%{shortcommit}
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
* Sat Oct 31 2026 Greenbugx <jesuschetia.gg@gmail.com> - 0.1.0^20261031git0abb051-1
- Initial Fedora package (snapshot of commit 0abb051)
