// Fast, cross-platform checks. Native desktop compilation and browser workflows
// are explicit extra checks; see docs/development.md for prerequisites.
for (const command of [
  ['bun', 'run', 'check:web'],
  ['bun', 'run', 'test:web'],
  ['bun', 'run', 'build:web'],
  ['cargo', 'fmt', '--all', '--', '--check'],
  ['cargo', 'clippy', '--all-targets', '--', '-D', 'warnings'],
  ['cargo', 'test', '--all-targets'],
]) {
  const result = Bun.spawnSync(command, {
    stdout: 'inherit',
    stderr: 'inherit',
  });
  if (result.exitCode !== 0) process.exit(result.exitCode);
}
