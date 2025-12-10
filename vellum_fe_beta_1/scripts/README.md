# Scripts

Utility scripts for maintaining the Profanitui repository.

## Automatic Wiki Sync (Recommended)

**The wiki syncs automatically via GitHub Actions!**

When you push changes to `/wiki/` in the main branch, a GitHub Action automatically syncs them to the GitHub Wiki within seconds.

**Workflow:**
1. Edit wiki files in `/wiki/` directory
2. Commit and push to main: `git add wiki/ && git commit -m "Update docs" && git push`
3. **That's it!** GitHub Action handles the rest automatically

**View the action:** https://github.com/Nisugi/Profanitui/actions/workflows/sync-wiki.yml

## Manual Wiki Sync (Optional)

If you need to sync manually (e.g., testing locally), use these scripts:

### sync-wiki.sh / sync-wiki.ps1

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

**When to use manual sync:**
- Testing wiki changes locally before pushing
- Emergency sync if GitHub Actions are down
- Syncing from a fork without Actions enabled

**Example:**
```bash
# Edit wiki files
vim wiki/Quick-Start.md

# Test sync locally (optional)
./scripts/sync-wiki.sh "Test update"

# Or just commit and push (recommended - auto-syncs via Actions)
git add wiki/Quick-Start.md
git commit -m "Update quick start guide"
git push  # ← GitHub Action syncs automatically!
```

## Source of Truth

The `/wiki/` directory in the main repository is the **source of truth**.

The GitHub Wiki at https://github.com/Nisugi/Profanitui/wiki is a **published copy** for easier browsing with sidebar navigation.

## Notes

- ✅ **Automatic sync enabled** - Just push to main branch
- The `.wiki-repo/` directory is gitignored (temporary clone for manual sync)
- Manual scripts are optional - primarily for local testing
- Always commit wiki changes to the main repo first
- GitHub Action runs on every push to `wiki/**` in main branch
