// By default each module in the tests folder is its own binary.
// This is a bit annoying in that far that it adds a lot of link time for no good reason
// since we practically always want to run all the tests.
//
// There's an easy workaround though:
// By having only a single top level module, everything becomes a single binary again!

mod src;
