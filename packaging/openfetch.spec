# RPM spec for Fedora / openSUSE (build with: rpmbuild -ba packaging/openfetch.spec)
Name:           openfetch
Version:        0.4.0
Release:        1%{?dist}
Summary:        Fast, customisable system information and visualisation tool
License:        MIT
URL:            https://github.com/Mayaiscool01/Openfetch
Source0:        %{name}-%{version}.tar.gz
BuildRequires:  cargo
BuildRequires:  rust

%description
OpenFetch shows system information beside a distro logo in the terminal, with
themes, a TOML configuration file, JSON output and optional animated logos.

%prep
%autosetup

%build
cargo build --release --locked

%check
cargo test --release --locked

%install
install -Dm755 target/release/openfetch %{buildroot}%{_bindir}/openfetch
install -Dm644 src/default_config.toml %{buildroot}%{_datadir}/openfetch/config.toml
install -Dm644 LICENSE %{buildroot}%{_licensedir}/%{name}/LICENSE

%files
%license LICENSE
%{_bindir}/openfetch
%{_datadir}/openfetch/config.toml

%changelog
* Tue Nov 10 2026 Mayaiscool01 - 0.4.0-1
- Initial package
