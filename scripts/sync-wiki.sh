#!/bin/bash
# sync-wiki.sh - Sync /wiki directory to GitHub Wiki repository
#
# Usage: ./scripts/sync-wiki.sh [message]
#
# This script syncs the /wiki directory to the GitHub Wiki repository.
# The /wiki directory is the source of truth, version-controlled with the main repo.

set -e  # Exit on error

WIKI_DIR="wiki"
WIKI_REPO_DIR=".wiki-repo"
WIKI_REPO_URL="https://github.com/Nisugi/Profanitui.wiki.git"

# Default commit message
MESSAGE="${1:-Update wiki documentation}"

echo "=== Profanitui Wiki Sync ==="
echo ""

# Check if wiki directory exists
if [ ! -d "$WIKI_DIR" ]; then
    echo "Error: $WIKI_DIR directory not found!"
    exit 1
fi

# Clone or update wiki repository
if [ ! -d "$WIKI_REPO_DIR" ]; then
    echo "Cloning wiki repository..."
    git clone "$WIKI_REPO_URL" "$WIKI_REPO_DIR"
else
    echo "Updating wiki repository..."
    cd "$WIKI_REPO_DIR"
    git pull origin master
    cd ..
fi

echo ""
echo "Syncing wiki files..."

# Remove old files (except .git)
find "$WIKI_REPO_DIR" -mindepth 1 -maxdepth 1 ! -name '.git' -exec rm -rf {} +

# Copy all wiki files
cp -r "$WIKI_DIR"/* "$WIKI_REPO_DIR"/

# Commit and push
cd "$WIKI_REPO_DIR"

# Check if there are changes
if git diff --quiet && git diff --cached --quiet; then
    echo ""
    echo "✓ No changes to sync"
    exit 0
fi

echo ""
echo "Committing changes..."
git add -A
git commit -m "$MESSAGE"

echo ""
echo "Pushing to GitHub Wiki..."
git push origin master

echo ""
echo "✓ Wiki synced successfully!"
echo ""
echo "View at: https://github.com/Nisugi/Profanitui/wiki"
