#!/bin/sh
set -eu
cd "$(dirname "$0")/.."
export CARGO_HOME="$PWD/.cargo-home"
cargo fetch --manifest-path Cargo.toml
curl --fail --location 'https://raw.githubusercontent.com/google/fonts/main/ofl/cormorantgaramond/CormorantGaramond%5Bwght%5D.ttf' -o assets/fonts/CormorantGaramond.ttf.tmp
mv assets/fonts/CormorantGaramond.ttf.tmp assets/fonts/CormorantGaramond.ttf
curl --fail --location 'https://raw.githubusercontent.com/google/fonts/main/ofl/cormorantgaramond/OFL.txt' -o assets/fonts/CormorantGaramond-OFL.txt
