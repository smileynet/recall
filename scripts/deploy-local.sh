#!/usr/bin/env bash
# deploy-local.sh — Build and deploy recall locally (macOS/Linux)
#
# Usage: ./scripts/deploy-local.sh [--skip-tests]
set -euo pipefail

SKIP_TESTS=false
for arg in "$@"; do
    case "$arg" in
        --skip-tests) SKIP_TESTS=true ;;
        *) echo "Unknown arg: $arg"; exit 1 ;;
    esac
done

REPO_ROOT="$(cd "$(dirname "$0")/.." && pwd)"
BIN_DIR="${CARGO_HOME:-$HOME/.cargo}/bin"
TARGET="$BIN_DIR/recall"
BACKUP="$TARGET.prev"

echo "recall deploy-local ($(uname -s))"
echo "  repo: $REPO_ROOT"
echo "  target: $TARGET"

# 1. Tests (gate)
if [[ "$SKIP_TESTS" != "true" ]]; then
    echo -e "\nRunning tests..."
    (cd "$REPO_ROOT" && cargo test --lib)
fi

# 2. Build
echo -e "\nBuilding release (--locked)..."
(cd "$REPO_ROOT" && cargo build --release --locked)

# 3. Backup previous binary
if [[ -f "$TARGET" ]]; then
    cp "$TARGET" "$BACKUP"
fi

# 4. Deploy (atomic via mv on same filesystem)
echo -e "\nDeploying..."
cp "$REPO_ROOT/target/release/recall" "$TARGET.new"
chmod +x "$TARGET.new"
mv "$TARGET.new" "$TARGET"

# 5. Verify
version=$("$TARGET" --version)
if [[ $? -ne 0 ]]; then
    echo "ERROR: Verification failed — rolling back"
    if [[ -f "$BACKUP" ]]; then mv "$BACKUP" "$TARGET"; fi
    exit 1
fi
echo "Installed: $version"

# 6. Smoke test
echo -e "\nHealth check..."
"$TARGET" health || echo "WARNING: health check returned non-zero (may be stale data)"

# 7. Check scheduled ingestion
echo ""
if [[ "$(uname)" == "Darwin" ]]; then
    plist="$HOME/Library/LaunchAgents/com.recall.ingest.plist"
    if [[ -f "$plist" ]]; then
        loaded=$(launchctl list 2>/dev/null | grep recall || true)
        if [[ -n "$loaded" ]]; then
            echo "LaunchAgent: loaded"
        else
            echo "LaunchAgent: plist exists but not loaded"
            echo "  launchctl load $plist"
        fi
    else
        echo "NOTE: No LaunchAgent configured. To schedule ingestion:"
        echo "  Create $plist with:"
        echo "    ProgramArguments: [\"$TARGET\", \"ingest\"]"
        echo "    StartInterval: 1800  (every 30 min)"
    fi
else
    if systemctl --user is-active recall-ingest.timer &>/dev/null; then
        echo "Systemd timer: active"
        systemctl --user status recall-ingest.timer --no-pager 2>/dev/null | head -3
    elif crontab -l 2>/dev/null | grep -q recall; then
        echo "Cron entry:"
        crontab -l 2>/dev/null | grep recall
    else
        echo "NOTE: No scheduled ingestion found. Options:"
        echo "  Cron:    */30 * * * * $TARGET ingest"
        echo "  Systemd: ~/.config/systemd/user/recall-ingest.{service,timer}"
    fi
fi

echo -e "\nDone."
