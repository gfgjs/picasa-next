@echo off
echo =======================================================
echo Chinese-CLIP ViT-Large to ONNX Conversion Script
echo =======================================================
echo.

:: Check if Python is installed
py --version >nul 2>&1
if %ERRORLEVEL% NEQ 0 (
    echo [ERROR] Python is not installed or not in PATH!
    echo Please install Python 3.10+ from the Microsoft Store or python.org, then try again.
    pause
    exit /b 1
)

:: Set paths
set SCRIPT_DIR=%~dp0
:: Find models dir based on standard location or APPDATA
set MODELS_DIR=C:\Users\gf\AppData\Roaming\com.scrollery.app\models
set PT_FILE=%MODELS_DIR%\clip_cn_vit-l-14.pt

if not exist "%PT_FILE%" (
    echo [ERROR] Could not find clip_cn_vit-l-14.pt in %MODELS_DIR%
    echo Please make sure you placed the downloaded .pt file in the models folder.
    pause
    exit /b 1
)

echo [1/3] Setting up Python virtual environment...
if not exist "venv" (
    py -m venv venv
)
call venv\Scripts\activate

echo [2/3] Installing PyTorch and ONNX dependencies...
python -m pip install --upgrade pip
pip install torch torchvision torchaudio onnx onnxscript --upgrade --index-url https://download.pytorch.org/whl/cpu

echo [3/3] Cloning OFA-Sys/Chinese-CLIP repository for conversion scripts...
if not exist "Chinese-CLIP" (
    git clone https://github.com/OFA-Sys/Chinese-CLIP.git
)

:: ==============================================================================
:: [IMPORTANT MANUAL PATCH INSTRUCTION / 重要手动修改指南]
::
:: [EN] If you are running this in a fresh environment where 'Chinese-CLIP' was just cloned,
::      you MUST manually edit the Python file before the export step will work!
:: [CN] 如果您在一个全新的环境中运行此脚本（刚刚全新 Clone 了 Chinese-CLIP），
::      在执行导出步骤前，您必须手动修改 Python 源码，否则一定会报错！
:: 
:: File to edit / 需要修改的文件: 
:: Chinese-CLIP\cn_clip\deploy\pytorch_to_onnx.py
:: 
:: Why? / 为什么？
:: [EN] The original script attempts to convert FP32 models to FP16 models. This uses 
::      a dependency called `onnxmltools`, which fundamentally conflicts with modern 
::      PyTorch/ONNX versions and causes 'mapping' and 'unflatten' crashes.
:: [CN] 原版脚本会尝试将模型转换为 FP16（半精度）。这会引入对 `onnxmltools` 的依赖，
::      而该库与最新版的 PyTorch 和 ONNX 存在根本性的底层冲突，会导致崩溃。
:: 
:: What to do / 怎么做：
:: Open `pytorch_to_onnx.py` and completely comment out/delete:
:: 打开 `pytorch_to_onnx.py` 文件，将以下代码彻底注释掉或删除：
::
:: 1. The import at the top / 顶部的 import 引入: 
::    `from onnxmltools.utils import convert_float_to_float16`
:: 2. The entire FP16 text conversion block (around line 125) / 整个 FP16 文本模型转换代码块 (约 125 行):
::    `text_fp16_onnx_path = ...` down to `convert_attribute=True)`
:: 3. The entire FP16 vision conversion block (around line 155) / 整个 FP16 图像模型转换代码块 (约 155 行):
::    `vision_fp16_onnx_path = ...` down to `convert_attribute=True)`
:: 
:: [EN] If you don't do this, the script will crash at the end. Since we only need FP32 
::      for Scrollery desktop inference, skipping FP16 avoids the dependency hell entirely.
:: [CN] 如果不这么做，脚本会在最后阶段崩溃。因为 Scrollery 在电脑端推理只需要 FP32
::      原精度模型，跳过 FP16 转换可以帮我们完美绕开依赖地狱。
:: ==============================================================================

echo.
echo Starting ONNX export...
set PYTHONPATH=%PYTHONPATH%;%SCRIPT_DIR%Chinese-CLIP;%SCRIPT_DIR%Chinese-CLIP\cn_clip
set PYTHONIOENCODING=utf-8

python Chinese-CLIP\cn_clip\deploy\pytorch_to_onnx.py ^
    --model-arch ViT-L-14 ^
    --pytorch-ckpt-path "%PT_FILE%" ^
    --save-onnx-path "%MODELS_DIR%\cn-clip-vit-l14" ^
    --convert-text ^
    --convert-vision

if %ERRORLEVEL% EQU 0 (
    echo.
    echo =======================================================
    echo [SUCCESS] ONNX models generated successfully!
    echo Look for cn-clip-vit-l14.img.fp32.onnx and cn-clip-vit-l14.txt.fp32.onnx in %MODELS_DIR%
    echo =======================================================
) else (
    echo.
    echo [FAILED] Conversion failed. Please check the error messages above.
)
pause
