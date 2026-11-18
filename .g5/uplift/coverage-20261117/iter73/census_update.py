#!/usr/bin/env python3
"""Iteration73 census regrade of devtools.generate.resources (new body) and devtools.authoring.generate#9 under
rule 4a. Run from the repository root; then run census.py count."""
import json
from pathlib import Path

root = Path(__file__).resolve().parents[4]
path = root / '.g5/uplift/coverage-20261117/census/blocks/features__developer-tools.json'
data = json.loads(path.read_text())
G = '.g5/Verus_verification/coverage-20261117/iter73/grouping.rs::'
I = '.g5/Verus_verification/coverage-20261117/iter73/instances.rs::'
U = '.g5/uplift/coverage-20261117/iter73/'
stamp = (root / U / 'native/LATEST').read_text().strip()
EV = [U + 'run.json', U + 'proofs/models.json', U + 'replay/receipt.json', U + f'native/{stamp}/observations.json',
      U + 'predictions.md']
BASIS = 'iteration-73 rule-4a regrade: '
rec = data['blocks']['devtools.generate.resources']
old = {o['id']: o for o in rec['obligations']}

def ob(n, units, text, cases, observation, grade, check, models, basis, support=None, gap=None, note=None,
       evidence=EV, composed=None, assessed='iteration-73'):
    o = {'id': f'devtools.generate.resources#{n}', 'units': units, 'text': text, 'cases': cases,
         'observation': observation, 'grade': grade, 'check': check, 'models': models, 'evidence': evidence,
         'basis': basis, 'assessed_in': assessed}
    for k, v in (('support', support), ('gap', gap), ('note', note), ('composed', composed)):
        if v:
            o[k] = v
    return o

obls = [
 ob(16, ['P1'], 'generate writes a module.xml that declares the source folder\'s files as resources: one package resource per group of classes, one resource per other routine, include file or package-less class, and UnitTest entries for the tests folder.',
    'packaged classes, leftovers, tests-folder classes (m1, m4)', 'written resources and UnitTest entries',
    'full', 'proved', [G + 'generate', G + 'add_packages', G + 'singles'],
    BASIS + 'composed from #7 (one package resource per group), #10 (leftovers one by one) and #2 (UnitTest entries); generate is their concatenation, so the lead holds exactly when the three do.',
    support='composition', composed=['devtools.generate.resources#7', 'devtools.generate.resources#10', 'devtools.generate.resources#2']),
 ob(15, ['P1'], 'If a module.xml already exists, generate first renames it to module_<timestamp>.xml before writing the new one.',
    'existing module.xml (m8) vs none (m7)', 'backup file count and content; content of the new module.xml',
    'partial', 'proved', [G + 'save_file', G + 'save_keeps_previous', I + 'm8_disk', I + 'm7_disk'],
    BASIS + 'the Exists branch of SaveFile is modeled (save_file); control no-backup is rejected by save_keeps_previous; native m8 has one backup holding the old content and m7 none; flip m8.backup.old rejected.',
    support='control',
    gap='The backup name module_<%PosixTime>.xml (observed module_1154716478026513909.xml) is native-only, and the model represents the backup as a list entry, not a file name. A failed rename (throws, nothing written) is unmodeled.'),
 old['devtools.generate.resources#3'] | {'units': ['B1'], 'models': [G + 'generate', G + 'missing_source_writes_nothing'], 'evidence': EV,
    'basis': BASIS + 'full, general theorem missing_source_writes_nothing; control missing-source-continues rejected by the general theorems alone; native m6.', 'assessed_in': 'iteration-73', 'support': 'control'},
 old['devtools.generate.resources#4'] | {'units': ['B1']},
 ob(1, ['B2'], 'Classes under the chosen tests folder stay out of application resources, even when that folder is inside the source folder.',
    'class under tests folder inside src (m4) vs tests outside src (m1) vs ordinary source class', 'membership in written application resources',
    'full', 'proved', [G + 'under_tests', G + 'kept', G + 'tests_folder_files_never_written', G + 'generate'],
    BASIS + 'the tests-folder test is computed from the tests answer and the file path (under_tests: the module-relative path starts with the tests folder), not an input. tests_folder_files_never_written holds for every file sequence; controls tests-not-excluded and tests-prefix-from-scan-folder are rejected by the general theorems alone; native m4 (inside) and m1 (outside).',
    support='control',
    note='Premise: ReadItems compares the normalized absolute path with the normalized tests directory (trailing "/"), which is a segment prefix test. Case-insensitive file systems and symbolic links are outside the model.'),
 ob(2, ['B2'], 'Tests are written as UnitTest entries with Phase="test", each named by the tests folder and directory and carrying the package its classes share.',
    'tests folder outside src (m1, Name /tests/) vs inside src (m4, Name /src/tests/); source classes get no UnitTest', 'UnitTest Name, processor, Package and Phase in module.xml and after load',
    'full', 'proved', [G + 'add_packages', G + 'test_entry', G + 'test_class_has_unittest_entry', G + 'source_listing_adds_no_unittest', G + 'group_characterized'],
    BASIS + 'TestEntry now carries Name (tests folder + directory), processor, Package and Phase, decided by the AddPackages tests-folder branch (add_packages). test_class_has_unittest_entry holds for every tests listing; controls tests-branch-swapped, unittest-name-without-folder and unittest-phase-other are rejected by the general theorems alone. Native m1 and m4 observe each field separately in module.xml and read the entry back after load; flips m4.unittest.name, m4.unittest.phase and m1.unittest.package are rejected.',
    support='control',
    note='Phase="test" and ProcessorClass UnitTest are constants inside the tests branch of AddPackages; the model decides them through that branch. UnitTest entries are kept out of the collapse because their GetSortKey includes the object reference (inspection; no native case has two UnitTest entries with one Name).'),
 ob(5, ['B3'], 'Files with unsupported extensions, and files whose class or routine name cannot be read, are skipped; a class whose name can be read is listed even if its body does not compile.',
    'unsupported .txt (m1); headerless class (Ig3) vs body-broken class (Ig2) (m5)', 'resource membership',
    'partial', 'proved', [G + 'supported', G + 'kept', G + 'unsupported_never_written', G + 'unlisted_never_written'],
    BASIS + 'the extension filter and the skip of unlisted files are general theorems; controls unsupported-kept and listing-ignored are rejected by the general theorems alone.',
    support='control',
    gap='Whether a name can be read is the provider result of $system.OBJ.Load list-only (File.listed), a fixture-fixed input. The headerless-versus-body-broken distinction the obligation states is observed natively (m5) but not decided by the model.'),
 old['devtools.generate.resources#6'] | {'units': ['B9']},
 ob(7, ['B4'], 'Classes are grouped into one package resource per (directory, top-level package); different top-level packages or directories give different resources.',
    'A.X + E.Y (two top levels), Id2 in x and y (two directories), A.B.C + A.B.D (one group)', 'written package entries',
    'full', 'proved', [G + 'in_group', G + 'group_key_separates', G + 'group_key_joins', G + 'class_in_own_group', I + 'grouping_names'],
    BASIS + 'group_key_separates and group_key_joins pin the key for every file sequence; controls top-level-only-key and directory-only-key are rejected by the general theorems alone (review-71 C2); native m1, m3.',
    support='control'),
 ob(8, ['B4'], 'Each group\'s resource is named by the deepest package all its classes share (classes A.B.C + A.B.D -> A.B.PKG; A.B.C + A.D.E -> A.PKG; A.B.C alone -> A.B.PKG).',
    'N1, N3, N4 neighbours; m1 Ia1, Ia2, Ia3', 'package entry names',
    'full', 'proved', [G + 'group', G + 'shared', G + 'group_characterized', I + 'grouping_names'],
    BASIS + 'group_characterized (deepest shared package, independent of file order) for every file sequence; controls name-top-level-only and seed-with-top-level rejected by the general theorems alone; native m1.',
    support='control'),
 ob(9, ['B5'], 'For a file stored in package folders, a resource\'s Directory is the folder that holds those package folders, relative to the source folder, and is left out when that folder is the source folder itself.',
    'cls/A/B/C.cls -> cls; x/Id1/B/C.cls -> x; a/b/Im7/Y.cls -> a/b; a root class -> none', 'Directory attribute',
    'full', 'proved', [G + 'directory', G + 'layout_ok', G + 'layout_directory', G + 'package_layout_round_trip'],
    BASIS + 'layout_directory and package_layout_round_trip hold for every file in package layout; control directory-keeps-whole-path rejected by the general theorems alone; native m1, m3, m7 (Directory a/b), m2e.',
    support='control'),
 ob(10, ['B6'], 'Routines, include files and classes without a package are written one by one under their own names.',
    'Ir69.MAC, Ii69.INC (m1), Ir2a.cls (m2a), IxbRtn, IxcInc (x2, x3)', 'individual resource entries',
    'full', 'proved', [G + 'singles', G + 'singles_exact'],
    BASIS + 'singles_exact for every file sequence; control drop-leftovers rejected by the general theorems alone; native m1, m2a, m2e, x1-x4.',
    support='control'),
 ob(11, ['B6'], 'Routines and package-less classes in the tests folder are written nowhere.',
    'Ih69.mac, Inopkg69.cls (m1); Ihelp69, Inopkg69b (m4)', 'absence from resources and UnitTest entries',
    'full', 'proved', [G + 'generate', G + 'tests_listing_never_resources', G + 'add_packages'],
    BASIS + 'tests_listing_never_resources holds for every tests listing (no longer instance-scoped); control tests-leftovers-kept rejected by the general theorems alone; native m1, m4.',
    support='control'),
 ob(12, ['B7'], 'module.xml keeps one resource per name.',
    'same name in two directories vs different names', 'count of entries per (name, extension)',
    'full', 'proved', [G + 'collapse', G + 'writer_one_entry_per_name', G + 'collapse_kept_is_last'],
    BASIS + 'writer_one_entry_per_name for every entry list; controls writer-keys-by-directory and collapse-keeps-first rejected by the general theorems alone; native m3, m7, m8 count fields.',
    support='control'),
 ob(13, ['B7'], 'As built (C:D72), when groups from two directories get the same package name, only one directory\'s resource is kept and the other directory\'s classes are missing from the module while load reports success.',
    'Id1.B in x and y (m3), Im7 in a and a/b (m7), Im8 in a and ab (m8) vs Id2.B in x with Id2.D in y', 'written entries, load result, classes compiled after load',
    'full', 'proved', [G + 'same_name_two_directories_keeps_one', G + 'collapse_earlier_key_loses', G + 'class_loads_when_no_clash', G + 'layout_loads', I + 'collapse_winners'],
    BASIS + 'as-built claim proved for every file sequence (same_name_two_directories_keeps_one); which directory is dropped is derived from the $order key strings (collapse_earlier_key_loses, premise keys_sorted); the no-clash neighbour loads every class (class_loads_when_no_clash). Controls writer-keys-by-directory, collapse-keeps-first, keys-sorted-descending rejected by the general theorems; key-separator controls rejected by instances. m7/m8 winners predicted before the run. Flips m3.Id1.B.PKG@y, m7.Im7.PKG@a, m8.Im8.PKG@ab, class.Id1.B.C, class.Im7.Y rejected.',
    support='control',
    note='Refutes the promise that every packaged application class is covered by a written resource. Premise: IRIS iterates non-numeric local subscripts in character-code order (keys_sorted), observed by m3, m7, m8.'),
 ob(14, ['B8'], 'As built (C:D73), load fails for a class without a package in the source folder when a cls folder also exists, and for a class file named with dots outside package folders.',
    'root class with cls folder (m2a) vs without (m2e); flat class file (m2b) vs package layout (m1)', 'load result',
    'full', 'proved', [G + 'reader_dir', G + 'reader_directory_rule', G + 'root_class_beside_cls_folder', G + 'load_ok', G + 'layout_loads'],
    BASIS + 'root_class_beside_cls_folder and reader_directory_rule hold for every file; control reader-root-before-ext-dir rejected by the general theorems alone; flips m2a.load.ok and m2b.load.ok rejected.',
    support='control',
    note='The flat-class-file case (m2b) is instance-scoped: it rests on the replayed native case, with the general reader rule; the obligation names that case only.'),
 ob(17, ['B8'], 'As built (C:D73), generate -export writes routines and include files flat into their extension folder, so one with dots in its name gets no Directory and load of the generated module fails.',
    'dotted routine Ixa.R.Rtn (x1) and include Ixd.I.Inc (x4) vs undotted IxbRtn (x2) and IxcInc (x3)', 'export file paths, written entries, load result, items present after load',
    'full', 'proved', [G + 'exported', G + 'export_layout', G + 'export_dotted_item_fails_load', G + 'export_round_trip_loads', I + 'export_round_trips'],
    BASIS + 'Expfilename placement transcribed (exported); export_layout and export_dotted_item_fails_load hold for every source tree; controls export-nests-all and export-classes-flat rejected by the general theorems alone. Native x1-x4 predicted before reading Expfilename and before the run; flips x1.load.ok, x4.load.ok, x2.load.ok, x1.file.Ixa.R.Rtn.flat, x1.Ixa.R.Rtn.MAC rejected.',
    support='control'),
]
rec['obligations'] = sorted(obls, key=lambda o: int(o['id'].split('#')[1]))
rec['units'] = {'P1': ['devtools.generate.resources#16', 'devtools.generate.resources#15'],
                'B1': ['devtools.generate.resources#3', 'devtools.generate.resources#4'],
                'B2': ['devtools.generate.resources#1', 'devtools.generate.resources#2'],
                'B3': ['devtools.generate.resources#5'],
                'B4': ['devtools.generate.resources#7', 'devtools.generate.resources#8'],
                'B5': ['devtools.generate.resources#9'],
                'B6': ['devtools.generate.resources#10', 'devtools.generate.resources#11'],
                'B7': ['devtools.generate.resources#12', 'devtools.generate.resources#13'],
                'B8': ['devtools.generate.resources#14', 'devtools.generate.resources#17'],
                'B9': ['devtools.generate.resources#6']}
inv = json.loads((root / '.g5/uplift/coverage-20261117/census/inventory.json').read_text())
rec['body_sha'] = next(b['body_sha'] for b in inv if b['id'] == 'devtools.generate.resources')

for o in data['blocks']['devtools.authoring.generate']['obligations']:
    if o['id'] == 'devtools.authoring.generate#9':
        o.update({
            'cases': 'exported class, dotted and undotted routine and include file (x1-x4); DFI item',
            'observation': 'export file path and resource present in manifest',
            'grade': 'partial', 'check': 'proved',
            'models': ['.g5/Verus_verification/consolidated/iter53/generator.rs::exported', G + 'exported', G + 'export_layout', G + 'singles_exact', G + 'class_written_before_collapse'],
            'evidence': ['.g5/uplift/consolidated/iter53/run.json'] + EV,
            'basis': BASIS + 'the placement step is now modeled (exported transcribes Expfilename) and exported classes, routines and include files become resources by the general grouping theorems; native x1-x4. iter53 exported() still decides which items are exported; the two models are joined by documentation, not one proof.',
            'assessed_in': 'iteration-73',
            'gap': 'DFI items are exported to dfi/ but ReadItems lists only cls, mac, inc and int, so by the model they never become resources (unsupported_never_written); this is source-only, with no native DFI case. The selection (iter53) and placement (iter73) models are composed by documentation only.',
        })
path.write_text(json.dumps(data, indent=1) + '\n')
print('updated', len(rec['obligations']), 'obligations')
