#!/usr/bin/env ruby
# Exercise the real formula's fetch method with both Homebrew helper APIs.
require "formulary"

formula_path = Pathname.new(__dir__).parent/"Formula/bionic-reader.rb"
formula = Formulary.factory(formula_path, :head)
commands = []
formula.define_singleton_method(:system) { |*arguments| commands << arguments }

# Older Homebrew versions do not provide std_cargo_fetch_args.
if formula.respond_to?(:std_cargo_fetch_args)
  formula.singleton_class.send(:undef_method, :std_cargo_fetch_args)
end
formula.fetch
unless commands.pop == ["cargo", "fetch", "--locked"]
  raise "Legacy Homebrew must fetch the locked Cargo dependencies"
end

# Preserve the helper's target selection when Homebrew provides it.
fetch_args = ["--locked", "--target", "aarch64-apple-darwin"]
formula.define_singleton_method(:std_cargo_fetch_args) { fetch_args }
formula.fetch
unless commands.pop == ["cargo", "fetch", *fetch_args]
  raise "Modern Homebrew must retain its standard Cargo fetch arguments"
end

puts "Source formula fetch passed with and without std_cargo_fetch_args."
