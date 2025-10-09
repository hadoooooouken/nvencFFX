@echo off
title Send to nvencFFX
mode con: lines=3 cols=60

set "APP_PATH=E:\ffmpeg\dist\nvencFFX.exe"

start "" "%APP_PATH%" "%~f1"