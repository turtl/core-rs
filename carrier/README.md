# Carrier

This is a simple library that wraps [crossbeam's MsQueue](https://aturon.github.io/crossbeam-doc/crossbeam/sync/struct.MsQueue.html)
to offer a simple in-memory messaging layer between various parts of an app.

Basically, anything that can speak C can send or receive messages. This is how
Turtl's core-rs communicates with whatever UI it's plugged into.

