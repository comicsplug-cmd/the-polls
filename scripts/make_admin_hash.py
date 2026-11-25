#!/usr/bin/env python3
"""Create an Argon2id admin password hash without echoing the password."""

from getpass import getpass
from argon2 import PasswordHasher

password = getpass("New admin password: ")
confirmation = getpass("Repeat password: ")
if len(password) < 16 or password != confirmation:
    raise SystemExit("Passwords must match and be at least 16 characters.")
print(PasswordHasher().hash(password))

