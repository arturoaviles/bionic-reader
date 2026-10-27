"""Run after installing the built bionic-reader wheel."""

from bionic_reader import Reader

reader = Reader()
print(reader.render("Hello world", format="html"))
print(reader.segments("Hello world"))
