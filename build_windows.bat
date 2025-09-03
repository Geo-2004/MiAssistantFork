@echo off
REM Build script for Windows using MSYS2 GCC
REM Uses msys2_shell.cmd to set up the environment properly

"C:\msys64\msys2_shell.cmd" -ucrt64 -c "cd /e/MiAssistantFork && gcc -Wall -O2 miasst_patched.c tiny-json/tiny-json.c -I. -o miasst_windows64.exe $(pkgconf --cflags --libs libusb-1.0 libcurl openssl) 2>&1"

if %errorlevel% neq 0 (
    echo Compilation failed with error code %errorlevel%
) else (
    echo Compilation successful. Executable: miasst_windows64.exe
)
