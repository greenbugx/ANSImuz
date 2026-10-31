%global pypi_name spotifyscraper

Name:           python-%{pypi_name}
Version:        3.9.2
Release:        1%{?dist}
Summary:        Python library for accessing public Spotify data

License:        MIT
URL:            https://github.com/AliAkhtari78/SpotifyScraper
Source0:        %{pypi_source %{pypi_name}}

BuildArch:      noarch

BuildRequires:  python3-devel
BuildRequires:  pyproject-rpm-macros

%global _description %{expand:
SpotifyScraper is a Python library for extracting public Spotify
data including tracks, albums, artists and playlists.}

%description %_description

%package -n python3-%{pypi_name}
Summary:        %{summary}

%description -n python3-%{pypi_name} %_description

%prep
%autosetup -n %{pypi_name}-%{version}

%generate_buildrequires
%pyproject_buildrequires -r

%build
%pyproject_wheel

%install
%pyproject_install
%pyproject_save_files spotify_scraper

%check
%pyproject_check_import -t

%files -n python3-%{pypi_name} -f %{pyproject_files}
%license LICENSE
%doc README.md
%{_bindir}/spotifyscraper
%{_bindir}/spotifyscraper-mcp

%changelog
* Sat Oct 31 2026 Greenbugx <jesuschetia.gg@gmail.com> - 3.9.2-1
- Initial Fedora package
