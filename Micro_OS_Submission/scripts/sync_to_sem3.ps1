# scripts/sync_to_sem3.ps1
# Synchronizes the VenOS project into Micro_OS_Submission/
# and pushes to https://github.com/Venu277/OS-Submissions-SEM3.git (branch main)
# maintaining a clean root folder structure.

param(
    [string]$CommitMessage = "sync(Micro_OS_Submission): update OS files from latest build"
)

$ErrorActionPreference = "Stop"
$root = $PSScriptRoot | Split-Path

Write-Host "==> Syncing project files into Micro_OS_Submission/..."
$dirs = @('src', 'scripts', 'builder', 'docs', 'tests', 'vmware', 'results', '.cargo', 'dist')
foreach ($d in $dirs) {
    $srcPath = Join-Path $root $d
    $destPath = Join-Path $root "Micro_OS_Submission"
    if (Test-Path $srcPath) {
        Copy-Item -Path $srcPath -Destination $destPath -Recurse -Force
    }
}

# Clean any copied build targets inside builder
$builderTarget = Join-Path $root "Micro_OS_Submission\builder\target"
if (Test-Path $builderTarget) {
    Remove-Item -Path $builderTarget -Recurse -Force
}

$files = @(
    'BOOT_TIMES.md', 'ca6_assignment.md', 'Cargo.lock', 'Cargo.toml',
    'laptop_hardware.txt', 'PROGRESS.md', 'README.md', 'run_venos.bat',
    'rust-toolchain.toml', 'x86_64-micro_os.json', '.gitignore'
)
foreach ($f in $files) {
    $srcFile = Join-Path $root $f
    if (Test-Path $srcFile) {
        Copy-Item -Path $srcFile -Destination (Join-Path $root "Micro_OS_Submission") -Force
    }
}

Write-Host "==> Generating clean root tree for OS-Submissions-SEM3 (main)..."
python -c "
import subprocess, sys

def run_cmd(args, input_data=None):
    p = subprocess.Popen(args, stdin=subprocess.PIPE if input_data else None, stdout=subprocess.PIPE, stderr=subprocess.PIPE)
    out, err = p.communicate(input_data)
    if p.returncode != 0:
        sys.stderr.write(err)
        sys.exit(p.returncode)
    return out.strip()

# 1. Stage Micro_OS_Submission changes in git index
subprocess.call(['git', 'add', '-A', 'Micro_OS_Submission'])

# 2. Get tree SHA for Micro_OS_Submission
micro_os_tree = run_cmd(['git', 'rev-parse', 'HEAD:Micro_OS_Submission'])

# 3. Get remote parent commit for sem3/main
parent_commit = run_cmd(['git', 'rev-parse', 'sem3/main'])

# 4. Get SHA of other required root items
entries_to_query = [
    'CA-3 deadlock-sim-report.pdf',
    'deadlock-sim',
    'investigation-report',
    'linux systems engineering challenge 25BTRBT052.pdf',
    'README.md'
]
lines = run_cmd(['git', 'ls-tree', parent_commit, '--'] + entries_to_query).splitlines()

# Build tree input
tree_entries = []
for line in lines:
    if line.strip():
        tree_entries.append(line + '\n')

# Add Micro_OS_Submission
tree_entries.append('040000 tree ' + micro_os_tree + '\tMicro_OS_Submission\n')

# Sort entries as git mktree expects
tree_data = ''.join(sorted(tree_entries, key=lambda x: x.split('\t')[1]))

# 5. Make tree
new_tree = run_cmd(['git', 'mktree'], tree_data)

# 6. Commit tree
msg = '''$CommitMessage'''
new_commit = run_cmd(['git', 'commit-tree', new_tree, '-p', parent_commit, '-m', msg])

print('New Commit SHA: ' + new_commit)

# 7. Update ref
run_cmd(['git', 'update-ref', 'refs/heads/sem3-main', new_commit])
"

Write-Host "==> Pushing to sem3 (main)..."
git push sem3 sem3-main:main

Write-Host "==> Successfully synchronized to OS-Submissions-SEM3/Micro_OS_Submission!"
