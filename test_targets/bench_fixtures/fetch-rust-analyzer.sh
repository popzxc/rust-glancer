#!/usr/bin/env bash
set -euo pipefail

repo_url="https://github.com/rust-lang/rust-analyzer.git"

# This revision is part of the benchmark definition. Keep it stable unless the
# benchmark target intentionally needs to move, and justify that change in review.
revision="b8458013c217be4fccefc4e4f194026fa04ab4ca"

script_dir="$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")" && pwd)"
fixture_dir="${script_dir}/rust-analyzer"

if [[ -e "${fixture_dir}" && ! -d "${fixture_dir}/.git" ]]; then
    echo "error: ${fixture_dir} exists but is not a git checkout" >&2
    exit 1
fi

if [[ ! -d "${fixture_dir}/.git" ]]; then
    git clone --filter=blob:none "${repo_url}" "${fixture_dir}"
fi

git -C "${fixture_dir}" fetch --filter=blob:none origin "${revision}"
git -C "${fixture_dir}" checkout --detach "${revision}"

(
    cd "${fixture_dir}"
    cargo fetch --locked --quiet
)
