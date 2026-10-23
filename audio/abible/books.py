"""The 66 books: USFM code, the repo's KJV text file, the 1769 title (eBible
USFM \\toc1), and the short name used in chapter announcements."""

BOOKS = [
    ("GEN", "old_testament/Genesis.txt", "The First Book of Moses, called Genesis", "Genesis"),
    ("EXO", "old_testament/Exodus.txt", "The Second Book of Moses, called Exodus", "Exodus"),
    ("LEV", "old_testament/Leviticus.txt", "The Third Book of Moses, called Leviticus", "Leviticus"),
    ("NUM", "old_testament/Numbers.txt", "The Fourth Book of Moses, called Numbers", "Numbers"),
    ("DEU", "old_testament/Deuteronomy.txt", "The Fifth Book of Moses, called Deuteronomy", "Deuteronomy"),
    ("JOS", "old_testament/Joshua.txt", "The Book of Joshua", "Joshua"),
    ("JDG", "old_testament/Judges.txt", "The Book of Judges", "Judges"),
    ("RUT", "old_testament/Ruth.txt", "The Book of Ruth", "Ruth"),
    ("1SA", "old_testament/First Samuel.txt",
     "The First Book of Samuel, otherwise called, The First Book of the Kings", "First Samuel"),
    ("2SA", "old_testament/Second Samuel.txt",
     "The Second Book of Samuel, otherwise called, The Second Book of the Kings", "Second Samuel"),
    ("1KI", "old_testament/First Kings.txt",
     "The First Book of the Kings, commonly called, The Third Book of the Kings", "First Kings"),
    ("2KI", "old_testament/Second Kings.txt",
     "The Second Book of the Kings, commonly called, The Fourth Book of the Kings", "Second Kings"),
    ("1CH", "old_testament/First Chronicles.txt", "The First Book of the Chronicles", "First Chronicles"),
    ("2CH", "old_testament/Second Chronicles.txt", "The Second Book of the Chronicles", "Second Chronicles"),
    ("EZR", "old_testament/Ezra.txt", "Ezra", "Ezra"),
    ("NEH", "old_testament/Nehemiah.txt", "The Book of Nehemiah", "Nehemiah"),
    ("EST", "old_testament/Esther.txt", "The Book of Esther", "Esther"),
    ("JOB", "old_testament/Job.txt", "The Book of Job", "Job"),
    ("PSA", "old_testament/Psalms.txt", "The Book of Psalms", "Psalms"),
    ("PRO", "old_testament/Proverbs.txt", "The Proverbs", "Proverbs"),
    ("ECC", "old_testament/Ecclesiastes.txt", "Ecclesiastes; or, the Preacher", "Ecclesiastes"),
    ("SNG", "old_testament/Song of Solomon.txt", "The Song of Solomon", "The Song of Solomon"),
    ("ISA", "old_testament/Isaiah.txt", "The Book of the Prophet Isaiah", "Isaiah"),
    ("JER", "old_testament/Jeremiah.txt", "The Book of the Prophet Jeremiah", "Jeremiah"),
    ("LAM", "old_testament/Lamentations.txt", "The Lamentations of Jeremiah", "Lamentations"),
    ("EZK", "old_testament/Ezekiel.txt", "The Book of the Prophet Ezekiel", "Ezekiel"),
    ("DAN", "old_testament/Daniel.txt", "The Book of Daniel", "Daniel"),
    ("HOS", "old_testament/Hosea.txt", "Hosea", "Hosea"),
    ("JOL", "old_testament/Joel.txt", "Joel", "Joel"),
    ("AMO", "old_testament/Amos.txt", "Amos", "Amos"),
    ("OBA", "old_testament/Obadiah.txt", "Obadiah", "Obadiah"),
    ("JON", "old_testament/Jonah.txt", "Jonah", "Jonah"),
    ("MIC", "old_testament/Micah.txt", "Micah", "Micah"),
    ("NAM", "old_testament/Nahum.txt", "Nahum", "Nahum"),
    ("HAB", "old_testament/Habakkuk.txt", "Habakkuk", "Habakkuk"),
    ("ZEP", "old_testament/Zephaniah.txt", "Zephaniah", "Zephaniah"),
    ("HAG", "old_testament/Haggai.txt", "Haggai", "Haggai"),
    ("ZEC", "old_testament/Zechariah.txt", "Zechariah", "Zechariah"),
    ("MAL", "old_testament/Malachi.txt", "Malachi", "Malachi"),
    ("MAT", "new_testament/Matthew.txt", "The Gospel according to Saint Matthew", "Matthew"),
    ("MRK", "new_testament/Mark.txt", "The Gospel according to Saint Mark", "Mark"),
    ("LUK", "new_testament/Luke.txt", "The Gospel according to Saint Luke", "Luke"),
    ("JHN", "new_testament/John.txt", "The Gospel according to Saint John", "John"),
    ("ACT", "new_testament/Acts.txt", "The Acts of the Apostles", "Acts"),
    ("ROM", "new_testament/Romans.txt", "The Epistle of Paul the Apostle to the Romans", "Romans"),
    ("1CO", "new_testament/First Corinthians.txt",
     "The First Epistle of Paul the Apostle to the Corinthians", "First Corinthians"),
    ("2CO", "new_testament/Second Corinthians.txt",
     "The Second Epistle of Paul the Apostle to the Corinthians", "Second Corinthians"),
    ("GAL", "new_testament/Galatians.txt", "The Epistle of Paul the Apostle to the Galatians", "Galatians"),
    ("EPH", "new_testament/Ephesians.txt", "The Epistle of Paul the Apostle to the Ephesians", "Ephesians"),
    ("PHP", "new_testament/Philippians.txt", "The Epistle of Paul the Apostle to the Philippians", "Philippians"),
    ("COL", "new_testament/Colossians.txt", "The Epistle of Paul the Apostle to the Colossians", "Colossians"),
    ("1TH", "new_testament/First Thessalonians.txt",
     "The First Epistle of Paul the Apostle to the Thessalonians", "First Thessalonians"),
    ("2TH", "new_testament/Second Thessalonians.txt",
     "The Second Epistle of Paul the Apostle to the Thessalonians", "Second Thessalonians"),
    ("1TI", "new_testament/First Timothy.txt", "The First Epistle of Paul the Apostle to Timothy", "First Timothy"),
    ("2TI", "new_testament/Second Timothy.txt", "The Second Epistle of Paul the Apostle to Timothy", "Second Timothy"),
    ("TIT", "new_testament/Titus.txt", "The Epistle of Paul the Apostle to Titus", "Titus"),
    ("PHM", "new_testament/Philemon.txt", "The Epistle of Paul the Apostle to Philemon", "Philemon"),
    ("HEB", "new_testament/Hebrews.txt", "The Epistle of Paul the Apostle to the Hebrews", "Hebrews"),
    ("JAS", "new_testament/James.txt", "The General Epistle of James", "James"),
    ("1PE", "new_testament/First Peter.txt", "The First Epistle General of Peter", "First Peter"),
    ("2PE", "new_testament/Second Peter.txt", "The Second Epistle General of Peter", "Second Peter"),
    ("1JN", "new_testament/First John.txt", "The First Epistle General of John", "First John"),
    ("2JN", "new_testament/Second John.txt", "The Second Epistle of John", "Second John"),
    ("3JN", "new_testament/Third John.txt", "The Third Epistle of John", "Third John"),
    ("JUD", "new_testament/Jude.txt", "The General Epistle of Jude", "Jude"),
    ("REV", "new_testament/Revelation.txt", "The Revelation of Saint John the Divine", "Revelation"),
]

CODES = [b[0] for b in BOOKS]
BY_CODE = {b[0]: b for b in BOOKS}
# The name the red-letter data (data/words_of_jesus.json) uses for each NT book.
RED_LETTER_NAME = {"MAT": "Matthew", "MRK": "Mark", "LUK": "Luke", "JHN": "John", "ACT": "Acts", "REV": "Revelation",
                   "1CO": "First Corinthians", "2CO": "Second Corinthians"}

_ONES = ("zero one two three four five six seven eight nine ten eleven twelve thirteen fourteen fifteen sixteen "
         "seventeen eighteen nineteen").split()
_TENS = "_ _ twenty thirty forty fifty sixty seventy eighty ninety".split()


def number_words(n: int) -> str:
    """119 -> 'one hundred and nineteen' (British, as the KJV counts); the
    recognizer writes the KJV's numbers as digits ("3,600"), up to millions."""
    if n < 20:
        return _ONES[n]
    if n < 100:
        return _TENS[n // 10] + ("" if n % 10 == 0 else "-" + _ONES[n % 10])
    if n < 1000:
        rest = n % 100
        return _ONES[n // 100] + " hundred" + ("" if rest == 0 else " and " + number_words(rest))
    for size, name in ((1_000_000, "million"), (1000, "thousand")):
        if n >= size:
            rest = n % size
            tail = "" if rest == 0 else (" and " if rest < 100 else " ") + number_words(rest)
            return number_words(n // size) + " " + name + tail


def announcement(code: str, chapter: int) -> str:
    """What the narrator says before a chapter: the book's title before its
    first chapter, then 'Genesis, chapter two.' (Psalms: 'Psalm two.')."""
    _, _, title, short = BY_CODE[code]
    label = ("Psalm " if code == "PSA" else "Chapter ") + number_words(chapter)
    if chapter == 1:
        return f"{title}. {label.capitalize()}."
    return f"{short}, {label.lower()}." if code != "PSA" else f"{label}."
