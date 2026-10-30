# Repository instructions

## Platform validation

- When working on Windows, build, run, and validate Windows only.
- Same constraint for other platforms. Only build, run, validate for the current platform. Don't try to cross compile.
- Do not attempt Linux or macOS validation from Windows, including through WSL,
  virtual machines, cross-compilation, or remotely triggered builds.
- Do not use Docker for this repository.
- Keep the CMake configuration portable and maintain build instructions for
  Windows, macOS, and Linux without running other platforms from Windows.
