# 🎮 MC Bedrock Control

<div align="center">

<img src="desktop/src-tauri/icons/128x128@2x.png" width="120">

### Administra tu servidor de Minecraft Bedrock desde una app sencilla para Windows.

![GitHub release](https://img.shields.io/github/v/release/arnaldo-puerta/MCBedrockControl?include_prereleases)
![Windows](https://img.shields.io/badge/Windows-10%2F11-blue)
![Tauri](https://img.shields.io/badge/Tauri-2-24C8DB?logo=tauri)
![Rust](https://img.shields.io/badge/Rust-backend-orange?logo=rust)
![React](https://img.shields.io/badge/React-19-61DAFB?logo=react)

</div>

> **⚠️ Aviso**
>
> MC Bedrock Control es un proyecto independiente y no está afiliado, respaldado ni desarrollado por Mojang Studios o Microsoft. Minecraft es una marca registrada de Microsoft Corporation.

---

## ✨ Qué puedes hacer

| Sección | Para qué sirve |
|---|---|
| 🖥️ **Servidor** | Iniciar, detener y reiniciar; ver jugadores, CPU, RAM y tiempo activo |
| 💻 **Consola** | Ver lo que pasa en tiempo real y escribir comandos |
| 👥 **Jugadores** | Lista de permitidos y expulsar jugadores |
| 🌐 **Conexiones** | Que tus amigos entren desde fuera de tu casa con Playit.gg |
| 🌍 **Mundos** | Importar, exportar, cambiar de mundo y borrar |
| 📦 **Packs y addons** | Añadir texturas y addons (.mcpack, .mcaddon, .zip) y activarlos |
| 🗂️ **Copias de seguridad** | Copias de seguridad sin apagar el servidor; también automáticas |
| ⚙️ **Configuración** | Nombre del servidor y del mundo, modo de juego, dificultad y más |
| ⬆️ **Actualización** | Detecta versiones nuevas y actualiza sin perder mundos ni packs |

La app descarga el servidor oficial por ti: no hace falta buscar ningún ZIP.

---

## 📦 Instalación

1. Ve a [Releases](https://github.com/arnaldo-puerta/MCBedrockControl/releases).
2. Descarga `MC Bedrock Control_..._x64-setup.exe` y ejecútalo.
3. Abre la app, elige una carpeta y pulsa **Instalar**.
4. Pulsa **Iniciar**.

Requisitos: Windows 10 u 11 (64 bits).

---

## 🛠 Desarrollo

Necesitas [Node.js](https://nodejs.org/) y [Rust](https://rustup.rs/).

```powershell
cd desktop
npm.cmd install
npm.cmd run tauri dev
```

Crear el instalador:

```powershell
npm.cmd run tauri build
```

Pruebas:

```powershell
cd src-tauri
cargo build --examples
cargo test
```

### Estructura

```text
desktop/
 ├── src/                 Interfaz (React + TypeScript + Tailwind)
 │   ├── components/      Una página por sección
 │   └── lib/             API, estado y utilidades
 └── src-tauri/           Backend (Rust)
     ├── src/bds/         Servidor: proceso, mundos, copias, packs, actualización
     ├── src/connect.rs   Integración con Playit.gg
     └── tests/           Pruebas con un servidor simulado
```

---

## 📄 Licencia

Este proyecto se distribuye bajo la **MC Bedrock Control License v1.0**. Consulta el archivo [LICENSE](LICENSE).

## 👨‍💻 Autor

**Arnaldo Puerta**

<div align="center">

Si el proyecto te resulta útil, considera darle una ⭐ en GitHub.

</div>
