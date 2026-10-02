import json
import sys


def numeric_release_id(release):
    if not isinstance(release, dict):
        raise ValueError("release response is not an object")
    release_id = release.get("id")
    if type(release_id) is not int or release_id <= 0:
        raise ValueError("matching release has no positive numeric id")
    return release_id


def created_release_id(response):
    return numeric_release_id(response)


def existing_release_id(pages, tag):
    if not isinstance(pages, list) or any(not isinstance(page, list) for page in pages):
        raise ValueError("release list response is not a list of pages")

    matches = [
        release
        for page in pages
        for release in page
        if isinstance(release, dict) and release.get("tag_name") == tag
    ]
    if len(matches) > 1:
        raise ValueError(f"multiple releases found for {tag}")
    if not matches:
        return None
    return numeric_release_id(matches[0])


def main(argv):
    if len(argv) not in (1, 2) or argv[0] not in ("created", "existing"):
        print("usage: release_id.py created | existing TAG", file=sys.stderr)
        return 2
    if (argv[0] == "created" and len(argv) != 1) or (argv[0] == "existing" and len(argv) != 2):
        print("usage: release_id.py created | existing TAG", file=sys.stderr)
        return 2

    try:
        response = json.load(sys.stdin)
        release_id = (
            created_release_id(response)
            if argv[0] == "created"
            else existing_release_id(response, argv[1])
        )
    except (json.JSONDecodeError, ValueError) as error:
        print(error, file=sys.stderr)
        return 1

    if release_id is not None:
        print(release_id)
    return 0


if __name__ == "__main__":
    sys.exit(main(sys.argv[1:]))
