@echo off
setlocal
where node >nul 2>&1
if errorlevel 1 (
    echo [ERRO] Node.js nao encontrado no PATH. Instale o Node.js v22+ em https://nodejs.org/
    pause
    exit /b 1
)
where cargo >nul 2>&1
if errorlevel 1 (
    echo [ERRO] Rust/Cargo nao encontrado no PATH. Instale o Rust em https://rustup.rs/
    pause
    exit /b 1
)
cd /d "%~dp0apps\desktop"
if not exist node_modules (
    echo Instalando dependencias do frontend...
    call npm install
)
call npm run tauri dev
