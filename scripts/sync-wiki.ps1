# sync-wiki.ps1 - Sync wiki to GitHub
param([string]$Message = "Update wiki documentation")

$WIKI_DIR = "wiki"
$WIKI_REPO_DIR = ".wiki-repo"
$WIKI_REPO_URL = "https://github.com/Nisugi/Profanitui.wiki.git"

Write-Host "=== Profanitui Wiki Sync ===" -ForegroundColor Cyan

if (-not (Test-Path $WIKI_DIR)) {
    Write-Host "Error: wiki directory not found!" -ForegroundColor Red
    exit 1
}

if (-not (Test-Path $WIKI_REPO_DIR)) {
    Write-Host "Cloning wiki repository..."
    git clone $WIKI_REPO_URL $WIKI_REPO_DIR 2>&1 | Out-Null
    if ($LASTEXITCODE -ne 0) {
        Write-Host ""
        Write-Host "Wiki not initialized yet. Please:" -ForegroundColor Yellow
        Write-Host "1. Go to https://github.com/Nisugi/Profanitui/wiki"
        Write-Host "2. Click 'Create the first page'"
        Write-Host "3. Title: Home, Content: # Welcome"
        Write-Host "4. Click Save, then run this script again"
        exit 1
    }
}
else {
    Set-Location $WIKI_REPO_DIR
    git pull origin master | Out-Null
    Set-Location ..
}

Write-Host "Syncing files..."
Get-ChildItem $WIKI_REPO_DIR -Exclude '.git' | Remove-Item -Recurse -Force
Copy-Item -Path "$WIKI_DIR\*" -Destination $WIKI_REPO_DIR -Recurse -Force

Set-Location $WIKI_REPO_DIR
$status = git status --porcelain

if ($status) {
    Write-Host "Committing changes..."
    git add -A
    git commit -m $Message | Out-Null
    Write-Host "Pushing to GitHub..."
    git push origin master
    Set-Location ..
    Write-Host ""
    Write-Host "SUCCESS! View at: https://github.com/Nisugi/Profanitui/wiki" -ForegroundColor Green
}
else {
    Set-Location ..
    Write-Host "No changes to sync" -ForegroundColor Yellow
}
