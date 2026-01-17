@echo off
setlocal

echo ======================================
echo ECH Workers RS - Qt GUI Build Script
echo ======================================
echo.

REM 检查 Qt 安装路径（根据实际安装修改）
if not defined Qt6_DIR (
    echo [ERROR] Qt6_DIR not set. Please set Qt6_DIR environment variable.
    echo Example: set Qt6_DIR=C:\Qt\6.7.0\msvc2019_64\lib\cmake\Qt6
    exit /b 1
)

echo [1/5] Checking directories...
if not exist build mkdir build

echo [2/5] Building Rust backend...
cd ..\ech-workers-rs
cargo build --release
if %ERRORLEVEL% neq 0 (
    echo [ERROR] Rust build failed
    exit /b 1
)

echo [3/5] Copying backend executable...
copy /Y target\release\ech-workers-rs.exe ..\qt-gui\build\ >nul
cd ..\qt-gui

echo [4/5] Building Qt GUI...
cd build
cmake .. -G "Visual Studio 17 2022" -DCMAKE_PREFIX_PATH=%Qt6_DIR%
if %ERRORLEVEL% neq 0 (
    echo [ERROR] CMake configuration failed
    exit /b 1
)

cmake --build . --config Release
if %ERRORLEVEL% neq 0 (
    echo [ERROR] Build failed
    exit /b 1
)

echo [5/5] Deploying Qt dependencies...
where windeployqt >nul 2>&1
if %ERRORLEVEL% neq 0 (
    echo [WARNING] windeployqt not found. Please add Qt bin directory to PATH.
) else (
    windeployqt Release\ech-workers-gui.exe --release --no-compiler-runtime
)

echo.
echo ======================================
echo Build completed successfully!
echo ======================================
echo Executable: build\Release\ech-workers-gui.exe
echo.

endlocal
