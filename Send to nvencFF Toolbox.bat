@echo off
title Send to nvencFF Toolbox
mode con: lines=3 cols=60

set "APP_PATH=E:\ffmpeg\dist\nvencFF Toolbox.exe"

start "" "%APP_PATH%" "%~f1"