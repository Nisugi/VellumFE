@echo off
REM Build script for VellumFE with vendored OpenSSL
REM
REM This script ensures Strawberry Perl is used for OpenSSL compilation
REM by setting proper PATH order and clearing MSYS environment variables.

REM Clear MSYS/Git Bash environment variables that interfere with build
set MSYSTEM=
set MSYS=
set GIT_EXEC_PATH=
set MINGW_PREFIX=

REM Ensure Strawberry Perl and NASM are found first
set PATH=C:\Strawberry\perl\bin;C:\Strawberry\c\bin;C:\Program Files\NASM;%PATH%

REM Run cargo build with all arguments passed through
cargo build %*
