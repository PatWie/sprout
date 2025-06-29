#!/bin/bash
set -e

SPROUT_BIN="/local/home/patwie/git/github.com/patwie/sprout2/target/release/sprout"
TEST_DIR="/tmp/sprout-test"
TEST_FILES_DIR="$HOME/sprout-testfiles"

echo "=== Symlinks Behavior Test ==="

# Clean up function
cleanup() {
    echo "Cleaning up..."
    rm -rf "$TEST_DIR" "$TEST_FILES_DIR"
    echo "Cleanup complete."
}

# Set up test environment
echo "Setting up test environment..."
rm -rf "$TEST_DIR" "$TEST_FILES_DIR"
mkdir -p "$TEST_DIR" "$TEST_FILES_DIR"

# Initialize sprout
cd "$TEST_DIR"
"$SPROUT_BIN" --sprout-path . -vvv init

# Create test files
echo "Creating test files..."
echo "bob content" > "$TEST_FILES_DIR/bob_file"
mkdir -p "$TEST_FILES_DIR/foo"
echo "baz content" > "$TEST_FILES_DIR/foo/baz"

echo "=== Test 1: Add individual file ==="
"$SPROUT_BIN" --sprout-path . -vvv symlinks add "$TEST_FILES_DIR/bob_file"

echo "=== Test 2: Check status (should show up-to-date) ==="
"$SPROUT_BIN" --sprout-path . -vvv symlinks check --all

echo "=== Test 3: Add file from subdirectory ==="
"$SPROUT_BIN" --sprout-path . -vvv symlinks add "$TEST_FILES_DIR/foo/baz"

echo "=== Test 4: Check status again ==="
"$SPROUT_BIN" --sprout-path . -vvv symlinks check --all

echo "=== Test 5: Delete a symlink to test D status ==="
rm "$TEST_FILES_DIR/bob_file"
"$SPROUT_BIN" --sprout-path . -vvv symlinks check

echo "=== Test 6: Restore missing symlink ==="
"$SPROUT_BIN" --sprout-path . -vvv symlinks restore

echo "=== Test 7: Verify restore worked ==="
cat "$TEST_FILES_DIR/bob_file"
"$SPROUT_BIN" --sprout-path . -vvv symlinks check

echo "=== Test 8: Undo a symlink ==="
"$SPROUT_BIN" --sprout-path . -vvv symlinks undo "$TEST_FILES_DIR/foo/baz"

echo "=== Test 9: Add directory recursively ==="
"$SPROUT_BIN" --sprout-path . -vvv symlinks add --recursive "$TEST_FILES_DIR/foo"

echo "=== Test 10: Final status check ==="
"$SPROUT_BIN" --sprout-path . -vvv symlinks check --all

echo "=== Test 11: Verify content still accessible ==="
cat "$TEST_FILES_DIR/bob_file"
cat "$TEST_FILES_DIR/foo/baz"

echo "=== All tests completed successfully! ==="

# Clean up
cleanup
