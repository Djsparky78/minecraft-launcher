# Ember Launcher

A lightweight Minecraft launcher foundation built with Tauri 2, React, TypeScript, and Rust.

## What is included

- A polished desktop launcher window
- Version selection and launch-state UI
- React-to-Rust communication through a Tauri command
- Responsive layout for compact desktop windows
- Tauri bundle configuration for Windows, macOS, and Linux

## Run locally

### Prerequisites

Install Node.js, Rust, and the platform dependencies listed in the [Tauri prerequisites guide](https://v2.tauri.app/start/prerequisites/).

### Start the desktop app

```bash
npm install
npm run tauri:dev
```

### Check the web interface only

```bash
npm run dev
```

## Current scope

This first milestone establishes the application shell. Microsoft authentication, Minecraft metadata, game installation, Java management, and process launching will be added in later milestones.

This project is not affiliated with Mojang Studios or Microsoft.
