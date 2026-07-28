#!/usr/bin/env bash
set -euo pipefail

# Ensure script is run from a git repository root
GIT_DIR=$(git rev-parse --git-dir 2>/dev/null) || {
    echo "Error: Not a git repository." >&2
    exit 1
}

HOOK_PATH="${GIT_DIR}/hooks/pre-commit"

echo "Installing pre-commit hook at ${HOOK_PATH}..."

mkdir -p "${GIT_DIR}/hooks"

cat << 'EOF' > "${HOOK_PATH}"
#!/usr/bin/env bash
set -euo pipefail

echo "🔍 Running pre-commit checks..."

echo "1/2 Checking code formatting (cargo fmt)..."
cargo fmt --all -- --check

echo "2/2 Running linter (cargo clippy)..."
cargo clippy --all-targets --all-features -- -D warnings

echo "✅ All pre-commit checks passed!"
EOF

chmod +x "${HOOK_PATH}"

echo "✅ Pre-commit hook installed successfully!"
