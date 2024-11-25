# Zim2Waj: Zim to Waj Converter

**A command-line tool to convert Zim archives to the Waj (Web Archive Jubako) format.**

Waj, a format based on Jubako, is designed for storing web content (static websites).
Like ZIM files, Waj archives can contain HTML, CSS, and JavaScript resources. The key advantage of Waj (as Zim) is that its content can be served directly by a local webserver without prior extraction, enhancing speed and convenience. Unlike ZIM, Waj omits metadata, full-text indexes, and title indexes, focusing solely on web resources.

However, Waj can store binary content in a separated pack, which allow to have text only archive than can be upgraded to
full archive only by adding the binary content.

## Installation

zim2waj relies on the `zim-rs` and `zim-sys` crates, which in turn depend on the `libzim` library. You must have `libzim` installed before compiling `zim2waj`.

**1. Install libzim:**

* **Linux (using package managers):**

  ```bash
  sudo dnf install libzim-devel  # Fedora/CentOS/RHEL
  ```

  or

  ```bash
  sudo apt-get install libzim-dev  # Debian/Ubuntu
  ```

* **Linux (or other systems) using pre-built binaries:** Download pre-built binaries from [https://download.openzim.org/release/libzim/](https://download.openzim.org/release/libzim/).  You will need to set the `PKG_CONFIG_PATH` and `LD_LIBRARY_PATH` environment variables to point to the correct directories containing the libzim library and header files respectively. For example:

  ```bash
  export PKG_CONFIG_PATH=/path/to/libzim/lib/pkgconfig:$PKG_CONFIG_PATH
  export LD_LIBRARY_PATH=/path/to/libzim/lib:$LD_LIBRARY_PATH
  ```

  Replace `/path/to/libzim` with the actual path to your extracted libzim directory.


**2. Install zim2waj:**

Once `libzim` is installed, install `zim2waj` using Cargo:

```bash
cargo install --git https://github.com/jubako/zim2waj
```


## Usage

**Basic Conversion:**

```bash
zim2waj <zim_file> --outfile <waj_file>
```

For optimal performance, increase the internal cluster cache of `libzim` to reduce cluster decompression overhead:

```bash
ZIM_CLUSTERCACHE=128 zim2waj <zim_file> --outfile <waj_file>
```

** Splitting Content:**

To separate binary content into a separate pack file, use the `--split` option:

```bash
zim2waj <zim_file> --outfile <waj_file> --split
```

This creates an additional file `<waj_file>.binary.jbkc`.  The main `<waj_file>` will function correctly even without the binary pack, albeit without the binary assets.  If you move the main Waj file, ensure the binary pack remains in the same directory.
 Alternatively, use `jbk locate` (from the `jubako` crate, installable via `cargo install jubako`) to update the binary pack location within the main Waj file.


## Contributing

Contributions are welcome! Please open an issue or submit a pull request.

## Sponsoring

I ([@mgautierfr](https://github.com/mgautierfr)) am a freelance developer. All jubako projects are created in my free time, which competes with my paid work.
If you want me to be able to spend more time on Jubako projects, please consider [sponsoring me](https://github.com/sponsors/jubako).
You can also donate on [liberapay](https://liberapay.com/jubako/donate) or [buy me a coffee](https://buymeacoffee.com/jubako).

## License

This project is licensed under the MIT License - see the [LICENSE-MIT](LICENSE-MIT) file for details.
