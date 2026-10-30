# Repository instructions

## Platform validation

- When working on Windows, build, run, and validate Windows only.
- Same constraint for other platforms. Only build, run, validate for the current platform. Don't try to cross compile.
- Do not attempt Linux or macOS validation from Windows, including through WSL,
  virtual machines, cross-compilation, or remotely triggered builds.
- Do not use Docker for this repository.
- Keep the CMake configuration portable and maintain build instructions for
  Windows, macOS, and Linux without running other platforms from Windows.

# Mandatory rules
 - No secret saved in this repo. No key, no certificate, no developer ID ...

 - No GPL code

 - All copyrights and LICENSES must be preserved when files are coming from other projects. The source must be documented with clear attribution.

 - No proprietary code. Open source if compatible with Apache license. 

- When you have finished with the apps that you have launched with computer use, quit them so that we do not have many apps opened