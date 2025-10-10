# Scripts

Utility scripts for maintaining the Profanitui repository.

## sync-wiki.sh / sync-wiki.ps1

Syncs the `/wiki` directory to the GitHub Wiki repository.

**Usage (Linux/Mac):**
```bash
./scripts/sync-wiki.sh ["commit message"]
```

**Usage (Windows PowerShell):**
```powershell
.\scripts\sync-wiki.ps1 ["commit message"]
```

**What it does:**
1. Clones the wiki repository (first run only) to `.wiki-repo/`
2. Copies all files from `/wiki/` to `.wiki-repo/`
3. Commits and pushes changes to GitHub Wiki

**When to use:**
- After updating any wiki pages in `/wiki/`
- Before creating a release
- When you want the GitHub Wiki to reflect latest documentation

**Source of Truth:**
The `/wiki/` directory in the main repository is the source of truth.
The GitHub Wiki is a published copy for easier browsing.

**Example:**
```bash
# Edit wiki files
vim wiki/Quick-Start.md

# Commit to main repo
git add wiki/Quick-Start.md
git commit -m "Update quick start guide"
git push

# Sync to GitHub Wiki
./scripts/sync-wiki.sh "Update quick start guide"
```

## Notes

- The `.wiki-repo/` directory is gitignored (it's a temporary clone)
- Always commit wiki changes to the main repo first
- The sync script will create `.wiki-repo/` on first run
- You need push access to the wiki repository
