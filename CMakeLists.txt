cmake_minimum_required(VERSION 3.24)
project(PteExplorer VERSION 0.1.0 LANGUAGES NONE)
find_program(CARGO_EXECUTABLE cargo REQUIRED)
option(PTE_GUI "Build the GPUI desktop application" ON)
set(PTE_CARGO_TARGET_DIR "${CMAKE_BINARY_DIR}/cargo" CACHE PATH "Cargo artifact directory")
set(PTE_GUI_ENV)
if(WIN32 AND PTE_GUI)
  # GPUI 0.2.2 treats all results from where.exe as one filename. Pass a single
  # compiler explicitly, including when several Windows SDKs are installed.
  if(DEFINED ENV{GPUI_FXC_PATH} AND EXISTS "$ENV{GPUI_FXC_PATH}")
    set(PTE_FXC_EXECUTABLE "$ENV{GPUI_FXC_PATH}" CACHE FILEPATH "Windows SDK shader compiler")
  endif()
  file(GLOB PTE_WINDOWS_SDK_BIN_DIRS LIST_DIRECTORIES TRUE
    "$ENV{ProgramFiles\(x86\)}/Windows Kits/10/bin/*/x64")
  list(SORT PTE_WINDOWS_SDK_BIN_DIRS COMPARE NATURAL ORDER DESCENDING)
  find_program(PTE_FXC_EXECUTABLE NAMES fxc.exe
    HINTS ${PTE_WINDOWS_SDK_BIN_DIRS}
    REQUIRED)
  list(APPEND PTE_GUI_ENV "GPUI_FXC_PATH=${PTE_FXC_EXECUTABLE}")
endif()
set(PTE_CARGO_FLAGS --locked)
if(NOT PTE_GUI)
  list(APPEND PTE_CARGO_FLAGS --no-default-features)
endif()
if(CMAKE_CONFIGURATION_TYPES)
  list(APPEND PTE_CARGO_FLAGS "$<IF:$<CONFIG:Release>,--release,--profile=dev>")
  set(PTE_PROFILE "$<IF:$<CONFIG:Release>,release,debug>")
elseif(CMAKE_BUILD_TYPE STREQUAL "Release")
  list(APPEND PTE_CARGO_FLAGS --release)
  set(PTE_PROFILE release)
else()
  set(PTE_PROFILE debug)
endif()
if(WIN32)
  set(PTE_EXE_SUFFIX .exe)
endif()
set(PTE_EXECUTABLE "${PTE_CARGO_TARGET_DIR}/${PTE_PROFILE}/pte-explorer${PTE_EXE_SUFFIX}")
add_custom_target(pte-explorer ALL
  COMMAND ${CMAKE_COMMAND} -E env "CARGO_TARGET_DIR=${PTE_CARGO_TARGET_DIR}"
    ${PTE_GUI_ENV}
    ${CARGO_EXECUTABLE} build ${PTE_CARGO_FLAGS}
  WORKING_DIRECTORY "${CMAKE_SOURCE_DIR}"
  BYPRODUCTS "${PTE_EXECUTABLE}"
  USES_TERMINAL VERBATIM)
add_custom_target(run
  COMMAND "${PTE_EXECUTABLE}"
  DEPENDS pte-explorer USES_TERMINAL VERBATIM)
enable_testing()
add_test(NAME core-tests
  COMMAND ${CMAKE_COMMAND} -E env "CARGO_TARGET_DIR=${PTE_CARGO_TARGET_DIR}/core-tests"
    ${CARGO_EXECUTABLE} test --locked --no-default-features
  WORKING_DIRECTORY "${CMAKE_SOURCE_DIR}")
if(PTE_GUI)
  add_test(NAME gui-interactions
    COMMAND ${CMAKE_COMMAND} -E env "CARGO_TARGET_DIR=${PTE_CARGO_TARGET_DIR}"
      ${PTE_GUI_ENV}
      ${CARGO_EXECUTABLE} test --locked --features ui-tests --bin pte-explorer
    WORKING_DIRECTORY "${CMAKE_SOURCE_DIR}")
endif()
install(PROGRAMS "${PTE_EXECUTABLE}" DESTINATION bin)
