Zim to Waj
==========


A tool to convert a Zim file into a Waj file.

Waj (Web Archive Jubako) is a format based on Jubako to store web content (aka static website).

As ZIM files, a Waj can contains web resources (html, css, js).
The content can be directly served as a local webserver without extracting the archive content first.

Contrarly to ZIM, Waj contains only web resources.
It doesn't contain a fulltext index nor a title index. So it is not possible to search content in it [TODO]

`zim2waj` tool read a ZIM archive and create a Waj archive, excluding Metadata, fulltext index and title index.


Installing zim2waj
------------------


To read zim content, zim2waj is based on [zim-rs](https://crates.io/crates/zim-rs) and [zim-sys](https://crates.io/crates/zim-sys)
which in turn, use [libzim](https://github.com/openzim/libzim) library.

You need to have libzim installed to be able to compile `zim2waj`

On linux, you can install libzim from standard package manangement:
```
$ sudo dnf install libzim-devel
```

or 

```
$ apt-get install libzim-dev
```


You can also use prebuild binaries from https://download.openzim.org/release/libzim/.
In this case, you will have to set `PKG_CONFIG_PATH` and `LD_LIBRARY_PATH` to point to correctly directories.


Then you can install `zim2waj` with:

```
$ cargo install --git https://github.com/jubako/zim2waj
```


Running zim2waj
---------------

Simply run:

```
$ zim2waj <zim_file> --outfile <waj_file>
```

For better performance, I advice you to increase the internal cluster cache of libzim to avoid some (a lot of) clusters uncompressions.

```
ZIM_CLUSTERCACHE=128 zim2waj <zim_file> --outfile <waj_file>
```

Splitting content
-----------------

Contrarly to Zim archive, Waj archive can store binary content (image, video) in a separated file (pack) than main content.
To do so, pass the `--split` option to `zim2waj`.

It will create an extra file `<waj_file>.binary.jbkc`. You can serve only `<waj_file>` without the binary content and you will
have a working waj file (without image obviously).

If you move `<waj_file>` be sure to keep the binary pack in the same directory.
Or use `jbk locate` (`$ cargo install jubako`) to update location of binary pack in `<waj_file>`.
