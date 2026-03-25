# academic-assistant

A Markdown project editor built with Rust, GTK4, libadwaita, GtkSourceView5, and WebKitGTK.

## Development Dependencies

To build and run this project on Debian/Ubuntu, you will need the Rust toolchain and several system development libraries.

### 1. Install Rust
If you haven't installed Rust yet, the recommended way is via [rustup](https://rustup.rs/):
```bash
curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh
```

### 2. Install System Dependencies (Debian/Ubuntu)

Run the following command to install all the necessary build tools and system headers for GTK4, WebKit6, libadwaita, and GtkSourceView5:

```bash
sudo apt update
sudo apt install \
    build-essential \
    pkg-config \
    libglib2.0-dev \
    libgtk-4-dev \
    libadwaita-1-dev \
    libwebkitgtk-6.0-dev \
    libgtksourceview-5-dev
```

> **Note**: These package names apply to recent versions of Debian (12+) and Ubuntu (22.04+). If you're on an older version of Ubuntu, you might need to use `libwebkit2gtk-4.1-dev` and adjust the `Cargo.toml` dependency to an older `webkit` crate, but `webkit6` (WebKitGTK 6.0) is the standard for modern GTK4 applications.

### 3. Build and Run

Once the dependencies are installed, you can compile and run the project using Cargo:

```bash
cargo build
cargo run
```
