# bundle-gen

## Introduction

`bundle-gen` is a tool for making bundles for the Atari VCS. A bundle
is a software package that the console user can choose to install, as
opposed to software that makes up the core platform, which is managed
externally. This tool takes a specification file and produces an
installable bundle for the VCS.

## Prerequisites

You will need Docker installed on your machine, and to have the
necessary permissions to use it. It's most convenient to use the
`make-bundle.sh` bash script to launch the tool - how to run this
depends on your system. On a modern Linux distribution, it is as
simple as adding it to your `PATH`.

## Tutorial

In this section we'll build the example bundles, and see how to use
`bundle-gen` in two standard scenarios.

### A Simple Example

We'll start with an example of using `bundle-gen` to create a real
Homebrew bundle.

We'll be building game contained in the Atari HTML Pong Example
repository, which can be found at

  https://github.com/atari-vcs/html-example-bundle

#### The Specification File

The HTML Pong example has a specification file,
`html-pong-example.yaml`, in the root of the repository. This is the
input for the `bundle-gen` tool. The contents of that file are:
```yaml
Name: "Html Pong Example"
HomebrewID: "HTMLPongExample"
Type: Game
Exec: res/index.html
Launcher: chrome

Build:
  VersionFile: version.txt
  Resources:
    - index.html
    - bounce.wav
    - explosion.wav
    - atari-controller.js
    - style.css
    - script.js
```
In general, you will want to put your specification file at the top
level of your project - this will be explained later.

There are two parts to this file. The first part, lines 1-5, describes
the game to the VCS - the necessary metadata - so that it can be
displayed in the right places to users, and so that the VCS knows how
to start it. The second part, the `Build` section, describes the files
that make up the bundle, and how to generate them (if necessary).

This is a very simple bundle. The first 5 lines say:
- List this bundle to users as "Html Pong Example" (its `Name`).
- This is a Homebrew bundle (it doesn't come from the store, users
  install it themselves via Homebrew), and its unique identifier
  (`HomebrewID`) is `HTMLPongExample`. This identifier is mostly there
  so that you don't end up with lots of copies of it, if you reinstall
  it. There's no issuer for these ideas, it's up to you to make it not
  clash with any other Homebrew bundles.
- It should be listed under games not under applications (`Type` is `Game`
  not `Application`).
- The VCS can launch it by passing the file in `res/index.html` inside
  the bundle to `chrome`. So this is a bundle containing a web page
  that is displayed by Google Chrome.

The `Build` section says:
- Take the version of the bundle from the file `version.txt` in the
  same directory as this YAML file (`VersionFile`).
- Include the following static list of files (`Resources`) from the
  current directory into the bundle, under `res/`.

#### Making the bundle

To make a bundle from this repository, we'll want to have the `bundle-gen`
scripts available. These scripts are written for `bash`, so you should
be able to run them in any suitable environment, although we test
exclusively on modern Linux machines.

If you have the bundle-gen scripts in your path, then in the top level
of the HTML Pong Example repository, you can simply run
```sh
make-bundle.sh html-pong-example.yaml
```
Otherwise you'll need to give the full path to `make-bundle.sh`. If
your system doesn't support shebangs (`#!` to select a script
interpreter, e.g. Windows), you may need to run it as:
```
bash /path/to/make-bundle.sh html-pong-example.yaml
```
The script is very simple, so if you're familiar with your
environment, you may find it easier to replace it with one of your own
(e.g. a Windows batch file).

The result of running this command will be a file
`html-pong-example_0.1.0.bundle` or similar (the version might change,
depending upon when you are reading this guide, `0.1.0` is current
at the time of writing).

You can now use Atari Homebrew to upload this bundle directly to your
VCS and play it.

### A More Complicated Example

Now that you have a grasp of the basics, it's time to look at a more
complicated example. `bundle-gen` primarily exists to make it easier
and less error-prone to create bundles containing native code.

We'll cover the Native Indy800 Example here, which can be
downloaded from GitHub

  https://github.com/atari-vcs/native-example-bundle

#### The Specification File

In the root of the native example bundle's repository there is a YAML
file, which can be used by `bundle-gen` to build this bundle. It has
the following contents:

```yaml
Name: "Native Indy 800 Example"
HomebrewID: "NativeIndy800Example"
Type: Game
Exec: bin/native

Build:
  VersionFile: version.txt
  RequiredPackages:
    - cmake
    - g++
  RequiredModules:
    - mod/sdl2-mixer.mod
  BuildCommand: build_script.sh
  Executables:
    - native
  Resources:
    - res/
```

The first section of this bundle is very similar to the previous
example. It's a Homebrew bundle, containing a `Game`. Notice that
there is no longer a `Launcher`: we are going to build a program that
is directly executable on the VCS. Also notice that the `Exec` part
says that the file to run will be under `bin/` (where conventionally
executable files are placed) rather than `res/` (where data is usually
placed).

The `Build` section contains most of the changes. This repository is
built with `cmake`, and the code is written in C++, so we install the
(Atari-provided) system packages for `cmake` and `g++` (the GNU C++
compiler) into our build environment using `RequiredPackages`. This
will not affect the host machine: the build runs in a Docker image.

##### Modules

Not every dependency you might have can be installed from the VCS
package repositories. For things you can't find there, you can use
_modules_. For example SDL2 mixer is not current provided in binary
form for the Atari VCS, and the native example depends on it. The
`RequiredModules` key gives an ordered list of scripts to run to set
up the build environment for your own build. It's structured this way
so that you can share and re-use modules for common libraries.
Remember to be careful to respect the licensing terms of libraries
that you link against.

The contents of `sdl2-mixer.mod` are:
```sh
#!/bin/bash

export DEBIAN_FRONTEND=noninteractive
apt-get install -y --no-install-recommends git

mkdir -p /usr/src
cd /usr/src
git clone -b release-2.0.4 https://github.com/libsdl-org/SDL_mixer.git

cd /usr/src/SDL_mixer
./configure --prefix=/usr
make -j$(nproc)
make install
```
This will clone the official SDL2 mixer repository at the 2.0.4
release tag (the latest at the time of writing) then build and install
it. Going through the script in sections:
- The first line is a shebang (`#!`) to tell a Linux system how to run
  this script. The build environment is always the Atari VCS Linux
  variant, regardless of what your host environment is, so you can
  rely on shebangs working.
- The next section installs `git` into the build environment, so that
  we can clone the repository. The extra options and variables around
  it are just to stop `apt-get` from ever prompting for user
  confirmation.
- The next section clones the source for SDL2 mixer into
  `/usr/src/SDL_mixer`.
- The last section configures SDL2 mixer to install into `/usr` (i.e. a
  system-wide install - this is safe because it only affects the
  temporary build environment), and to use as many cores as possible
  to do the build.

You can in principle build any dependencies you might have using
modules. You should prefer packages when they are available, because
they install much more quickly - the module has to run a build every
single time you rebuild the bundle, although you can mitigate some of
this for bigger libraries, by using a build directory under `/build`
that will persist between rebuilds - and because customisations for
the VCS may have been applied to Atari supplied packages.

##### The Build Command

Unlike our earlier, simpler example, there is now a build stage to
making our bundle. We need to do the build within the `bundle-gen`
build environment so that we get the right binary format and shared
library dependencies for the VCS. In this example, the build script is
just called `build_script.sh`. It has the following contents:

```sh
#!/bin/bash

cmake /source
make -j$(nproc)
```

As mentioned before, when we're building our bundle, we're running in
an Atari VCS environment in a Docker image. The provided script,
`make-bundle.sh` maps two host directories into that Docker image:
- `/source` is mapped to the directory containing the specification
  (YAML) file.
- `/build` is mapped to the current directory at the time you ran the
  script.
If you run `make-bundle.sh` from the same directory as your YAML file,
then both `/build` and `/source` are mapped to the same directory.

Your initial working directory in the build environment will be
`/build`, so the build script given here can be used to perform an out
of tree `cmake` build, just by making an empty directory and running
`make-bundle.sh` from there.

##### The Bundle Contents

Once the build script has completed, the final two sections of the
specification file will be used:
- `Executables` gives a list of native (ELF-format) programs to copy
  into the `bin/` directory of your bundle.
- `Resources` as before gives a list of files or directories to copy
  over into the `res/` directory of your bundle.

There are a couple of things to note here. All shared libraries that
your executables depend on from your build container, that are not
part of the standard VCS base system, will be added to your bundle in
`lib/` automatically. In the vast majority of cases, you can just
ignore this issue from a technical standpoint, although you must of
course always bear in mind the licensing terms of any third party
software you choose to distribute in your own bundles.

Directories in the list of resources are treated differently,
depending upon whether they have a trailing slash. A trailing slash
(as the example has) means to copy the contents of the directory into
`res/`; no trailing slash means to copy the directory itself. If we
removed the trailing slash in the example, our resulting bundle would
have a directory `res/res/`.

#### Making the bundle

As before we'll need to have the bundle-gen scripts available. The
native example is tested with an out of tree build. So after cloning
it, create a directory `build` in the top level
of the native example bundle repository, change into that directory,
and run
```sh
make-bundle.sh ../native-indy800-example.yaml
```

The result of running this command will be a file
`native-indy-800-example_0.1.0.bundle` or similar (the version might change,
depending upon when you are reading this guide, `0.1.0` is current
at the time of writing).

You can now use Atari Homebrew to upload this bundle directly to your
VCS and play it.

## Reference

This section provides a short-form overview of all the functionality
provided by `bundle-gen`.

### Invoking

`bundle-gen` is built into a Docker image suitable for creating binaries
for the Atari VCS: the Docker image has an entrypoint set to the
`bundle-gen` tool from this repository.

The included script `make-bundle.sh` will mount into the Docker image:
- Your current directory as `/build`.
- The directory containing the specification file as `/source`.

The only argument to either the Docker image or `make-bundle.sh` is a
specification file; this argument is mandatory.

Your `/source` directory will be mounted read-only, and your `/build`
directory will be mounted writable. For the common case of your
game or application's build system using an out of tree build, you
should invoke the `make-bundle.sh` from your build directory. For an
in-tree build, invoke it in the same location as the specification
file.

When run through the standard script, the `bundle-gen` tool will not
have access to directories above your specification file, so you
should usually place the bundle specification file in the top level of
your project. You can however modify the script, if needed.

### Packages

All the packages initially installed on the Docker image, and any
packages additional packages you request, are drawn from the Atari VCS
OS `apt` repositories. They are mostly a subset of the packages
available in Debian, so you can use Debian package searches online to
identify the names of the packages you will need. Packages listed
under `RequiredPackages` are installed before your modules (if any)
run, and before your build script. You should make sure that you are
aware of, and obey, the licensing terms of any packages you use. The
use of packages is optional.

### Modules

Modules provide an easy way to make use of software that isn't included
in the official Atari VCS OS repositories. Each module is just an
executable program. They are conventionally written for `bash`, but
they could be written in any language that you have installed into the
build image, and could even be executable programs if you know they
are binary compatible with the VCS environment (for example,
self-extracting launchers).

Note that modules are run on each and every build, and naive modules
can lead to long build times. If you want to speed things up, you need
to do any required caching of build products within the module
itself. You may also find it helpful to use a directory under `/build`
to store your sources and generated object files, to avoid repeating
compilation. Modules are intended to be self-contained, so that you
can re-use them between projects, and perhaps share them with other
developers. Note that you are responsible for honoring the license
terms of any software you install using modules.

To improve re-usability, it is wise to install any packages your
module requires within the module itself. This will make it easier to
transfer modules directly between projects, without needing to find
out each module's required environment: a module should establish its
own required environment.

The use of modules is optional.

### Build Scripts

A build script is not required. If present it will be run after the
the required packages and modules have been installed. It must be
executable. Its initial working directory will be `/build`, which is
writable, and (if using the standard script, `make-bundle.sh`) is the
directory from which the tool was launched.

You may find it helpful within your build script to generate a version
file, for example from your git repository tags, or otherwise. This is
supported: a version will not be assigned to your bundle until the
build script has run, and there is a field in the specification for
the file to read the version from.

### Specification File

Each bundle you want to generate has its own specification file, in
YAML format, which can be passed to `bundle-gen`. The contents of this
file are described in this section.

#### Bundle Metadata

The first part of the YAML specification file is very similar to the
`bundle.ini` file that each bundle contains.  It contains:
- `Name`: The human readable name of your program, which will be shown
  to users.
- `Type`: The kind of bundle you are making: `Game`, `Application` or
  `LauncherOnly`. Note that both games and applications can be
  launchers as well, but `LauncherOnly` titles will not be shown in
  the user's list of installed games on the dashboard (Homebrew
  bundles cannot be launchers, they can only use them).
- `StoreID`: For store bundles, the unique identifier obtained from
  Atari for your product. This is guaranteed unique.
- `HomebrewID`: For Homebrew bundles, an identifier you chose for your
  game or application; you're responsible for its uniqueness.
- `Exec`: Either the command to run, if you don't specify a launcher,
  or the arguments to provide to your chosen launcher.
- `Background`: For store bundles only, specify that this bundle runs
  in the background. Optional, defaults to false.
- `PreferXBoxMode`: Request that the VCS controllers be set to emulate
  XBox360 controllers before your program starts. You lose out on
  unique Atari functionality, but you might find it easier to get
  compatible behaviour with other platforms. Optional, defaults to false.
- `Launcher`: Request that another program be used to run your program,
  for example an emulator, or web browser. This is a tag name, and the
  VCS will match it to a launcher providing that tag amongst its list
  of `LauncherTags`.  Optional.
- `LauncherTags`: For store bundles only, instruct the VCS to use your
  bundle to launch games specifying any of the given list of launcher
  tags.
- `LauncherExec`: For store bundles only, the command to run when
  functioning as a launcher. The environment variable `BUNDLE_PATH`
  will point to the install location of the bundle you have been asked
  to run. The active home directory will be that bundle's home
  directory.

#### The Build Section

The Build section contains the instructions for generating files and
including them in your bundle. It has the following keys,
which are effectively worked through in order:

- First, the `RequiredPackages` key is read, and any listed packages are
  installed into the Docker image.

- Then the `RequiredModules` key is read, and all listed modules are
  run directly.

- Next, the `BuildCommand` key is read, and the script given there is
  found and executed.

- Now the `VersionFile` is read, and the version of the bundle
  identified. This is just plain text, and its contents will be
  trimmed at the ends for whitespace before becoming the version
  string

- A bundle archive is created with the same name stem as the YAML
  file, but with a version appended as found in the previous step.

- All the files listed under `Executables` are found and put into
  `bin/` in the bundle.

- All the libraries listed under `Libraries` are found and put into
  `lib/` in the bundle. You do not need to list system libraries, they
  will be discovered automatically.

- You can, if necessary, specify additional files whose dependencies
  you want to include from the build image using
  `ExtraElfFiles`. These files are not automatically copied into the
  archive, only their dependencies. This is only needed in rare
  circumstances, for things like dynamically loaded plugins that can't
  be identified automatically upfront, and can't be placed under
  `lib/` for some reason.

- All the shared system libraries on the Docker image that your
  bundle depends upon, which aren't available by default on the VCS,
  are found and put into `lib/` in the bundle.

- All the files listed under `Resources` are found and put into `res/`
  in the bundle. The lookup is the same as for `Executables`,
  `Libraries` and `BuildCommand`. A trailing slash on a directory
  means to copy only the contents, discarding the directory. Without a
  trailing slash, the directory itself will appear under `res/`.
