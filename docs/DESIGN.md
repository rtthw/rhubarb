
> [!WARNING]
> This document is a DRAFT. Information may be inaccurate or incomplete.

<details>
<summary>Table of Contents</summary>

- [Design](#design)
  - [Introduction](#introduction)
  - [Background](#background)
  - [Concepts](#concepts)

</details>

# Design

## Introduction

Modern computing is maliciously complex.

Much of this complexity is historical baggage. Operating systems must support hardware that is no longer in use and users that no longer exist. No operating system built for a desktop environment should support 32-bit CPUs. An immense amount of complexity can be completely removed quite easily.

Upwards of 95% of all code within modern operating systems is garbage that needs to be thrown out immediately. **It's time to build something cleaner.**

## Background

*This section outlines both the philosophy and historical reasoning behind many design choices made for Rhubarb.*

### Object-oriented programming is a failure.

The current consensus surrounding object-oriented programming (OOP) is that it has failed. It's far too slow for serious applications and the abstractions it encourages are verbose and unwieldy. In a sense this is true, but it's not the full story. The *modern understanding of OOP* is a failure, but what we see today is not what OOP could/should have been.

> OOP to me means only messaging, local retention and protection and hiding of state-process, and extreme late-binding of all things.
> - Alan Kay, 2003

During his [keynote speech at OOPSLA 1997](https://www.youtube.com/watch?v=oKg1hTOQXoY), Alan Kay makes the assertion, "at the very least, every object should have a URL." What he means by this is that we've been thinking too small when it comes to defining objects. They should be individual programs, applications, servers, and daemons. **Instantiation is invocation.**

### Not everything is a file.

*TODO: Explain how the Unix philosophy is flawed. The file abstraction doesn't work everywhere. Text is not the universal interface. `cat -v` considered harmful. `$PATH` is bad. Scripting is not robust.*

## Concepts

*This section outlines the core concepts of Rhubarb.*

### Executables are libraries.

Calling a library function is no different from invoking a subcommand. This means `lib::main(arg)` is no different from `exec("path/to/bin", args)`. Binaries are just relocatable object files. All linkage happens at run time.

### Resources are dependencies.

Resources can range from high-level abstractions (like "a window" or "a file") to individual devices (like "the PCI bus" or "hard disk 0:0").

A program can request access to a resource simply by depending on it. Depending on a resource can be as simple as attempting to access it (e.g. reading from a device's I/O port) or, if the resource is more abstract, linking to whatever executable/library provides it (e.g. the window manager).

### Deny by default.

The security philosophy behind modern operating systems is that if some code is running, then the user must want to run. Therefore, it should be trusted. As such, systems like Linux and Windows have an "allow by default" policy for granting process capabilities. By default, processes can do things like access the filesystem, read user data, and send/receive network packets unless the user goes through the arduous process of denying access to these resources. This is unacceptable behavior.

Rhubarb takes the opposite approach. Resource access is universally denied for all processes. The user has to manually grant permissions on a per-process basis.
