import os


def main():
    home = os.getenv("HOME")
    print(home)


def fetch():
    return os.getenv("HOME")


main()
