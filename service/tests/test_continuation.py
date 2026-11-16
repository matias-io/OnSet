"""Regressions for vocal clashes, mix windows, brief preferences, pins, replanning,
review priority, portable workspaces, cache relocation and cancellable planning."""
import copy
import json
import shutil
import threading
import numpy as np
import pytest
from conftest import analysis
from onset_service.transitions import options, windows
from onset_service.planning import build_plans, validate_plan, smoothness_value
from onset_service.structure import mix_windows
from onset_service.review import queue
from onset_service import portable


def track(i, bpm=120, key='A minor', vocals=None):
    t = {'id': i, 'title': i, 'revision': 'r1', 'duration': 120, 'bpm': bpm, 'key': key, 'genres': [], 'cues': [],
         'analysis': {'downbeats': list(range(0, 120, 2)),
                      'structure': {'windows': [{'id': 'intro', 'start': 0, 'end': 64, 'label': 'intro', 'vocals': 'unknown'},
                                                {'id': 'outro', 'start': 80, 'end': 120, 'label': 'outro', 'vocals': 'unknown'}]}}}
    if vocals is not None:
        t['analysis']['vocal_activity'] = list(vocals)
    return t


def test_measured_vocal_clash_removes_long_overlaps_but_keeps_cuts():
    a, b = track('a', vocals=[.8] * 60), track('b', vocals=[.8] * 60)
    values = options(a, b, limit=18)
    assert values and all(o['wall_seconds'] <= 4 or o['vocal_overlap']['state'] != 'clash' for o in values)
    assert not any(o['technique'] in {'long_blend', 'eq_swap'} for o in values)
    cut = next(o for o in values if o['technique'] == 'cut')
    assert cut['vocal_overlap']['outgoing_phrase_cut']
    assert any('still active at the cut' in w for w in cut['warnings'])


def test_measured_clear_vocals_replace_unverified_warning():
    a, b = track('a', vocals=[.8] * 60), track('b', vocals=[0.] * 60)
    blend = next(o for o in options(a, b, limit=18) if o['technique'] == 'long_blend')
    assert blend['vocal_overlap']['state'] == 'clear'
    assert 'Vocal overlap unverified' not in blend['warnings']
    unknown = next(o for o in options(track('a'), track('b'), limit=18) if o['technique'] == 'long_blend')
    assert unknown['vocal_overlap']['state'] == 'unknown' and 'Vocal overlap unverified' in unknown['warnings']


def test_brief_vocal_overlap_is_a_warning():
    vocals = [0.] * 60
    vocals[0] = vocals[40] = .9  # one vocal bar at the start of each exit window
    a, b = track('a', vocals=vocals), track('b', vocals=[.9] * 60)
    quick = [o for o in options(a, b, limit=18) if o['vocal_overlap']['state'] == 'brief']
    assert quick and all(any('Vocals overlap for about' in w for w in o['warnings']) for o in quick)


def test_mix_windows_need_steady_rhythm_and_runway():
    n = 32
    rhythm = np.r_[np.full(16, .8), np.full(16, .1)]
    result = mix_windows(rhythm, np.full(n, .5), np.zeros(n), [{'bar': 12, 'strength': .9}], np.arange(n) * 2., 64.)
    assert result and result[0]['start_bar'] == 0 and result[0]['end_bar'] == 12
    assert result[0]['runway_bars'] == 12 and result[0]['vocals'] is False
    assert mix_windows(np.full(n, .1), np.zeros(n), None, [], np.arange(n) * 2., 64.) == []


def test_mix_windows_join_transition_candidates():
    a = track('a')
    a['analysis']['structure']['mix_windows'] = [{'id': 'mix-44-56', 'start': 88, 'end': 112, 'start_bar': 44, 'end_bar': 56,
                                                  'label': 'mixable', 'score': .9, 'vocals': False, 'runway_bars': 12}]
    assert 'mix-44-56' in {w['id'] for w in windows(a, True)}
    a['overrides'] = {'sections': [{'start': 0, 'end': 120, 'label': 'groove'}]}
    assert 'mix-44-56' not in {w['id'] for w in windows(a, True)}


def test_transpose_makes_a_harmonic_blend_and_is_validated():
    a, b = track('a'), track('b', key='C# major')
    assert not any(o['technique'] == 'long_blend' for o in options(a, b, limit=18))
    brief = {'transpose': {'b': -1}}
    shifted = [o for o in options(a, b, brief, limit=18) if o['technique'] == 'long_blend']
    assert shifted and shifted[0]['in_transpose'] == -1
    assert 'key shift to -1 semitones' in shifted[0]['actions'][0]['action']
    plan = {'schema_version': 1, 'track_ids': ['a', 'b'], 'transition_ids': [shifted[0]['id']]}
    lookup = {shifted[0]['id']: shifted[0]}
    assert validate_plan(plan, [a, b], lookup, {'target_seconds': 200, 'tolerance_seconds': 200, 'transpose': {'b': -1}})['valid']
    with pytest.raises(ValueError, match='transpose'):
        validate_plan(plan, [a, b], lookup, {'target_seconds': 200, 'tolerance_seconds': 200})
    from onset_service.models import SetBrief
    with pytest.raises(ValueError, match='semitones'):
        SetBrief.parse({'transpose': {'b': .5}})


def test_smoothness_and_contrast_shift_plan_scores():
    cut = {'technique': 'cut', 'wall_seconds': 0}
    blend = {'technique': 'long_blend', 'wall_seconds': 32}
    a, b = track('a'), track('b')
    assert smoothness_value(blend, a, b) > 0 > smoothness_value(cut, a, b)
    profiles = [track('a'), track('b')]
    base = {'target_seconds': 200, 'tolerance_seconds': 200, 'must_play': ['a', 'b'], 'start': 'a'}
    smooth = build_plans(profiles, {**base, 'smoothness': 1, 'contrast': 0})
    sharp = build_plans(profiles, {**base, 'smoothness': 0, 'contrast': 1})
    lookup = {o['id']: o for o in smooth['transitions']}
    plan = smooth['plans'][0]
    same = next(p for p in sharp['plans'] if p['transition_ids'] == plan['transition_ids'])
    technique = lookup[plan['transition_ids'][0]]['technique']
    assert (plan['score'] > same['score']) == (technique not in {'cut', 'drop_swap', 'double_drop', 'backspin', 'echo_out', 'reverb_out', 'fade', 'tempo_bridge'})


def test_playlist_candidates():
    profiles = [track('a'), track('b'), track('c')]
    for p in profiles[:2]:
        p['rekordbox_playlists'] = ['Warmup']
    result = build_plans(profiles, {'target_seconds': 200, 'tolerance_seconds': 200, 'playlists': ['warmup']})
    assert result['plans'] and all(set(p['track_ids']) <= {'a', 'b'} for p in result['plans'])
    empty = build_plans(profiles, {'target_seconds': 200, 'tolerance_seconds': 200, 'playlists': ['Peak']})
    assert not empty['plans'] and any('playlists' in c for c in empty['conflicts'])


def test_pinned_transition_is_used_by_the_search():
    profiles = [track('a'), track('b'), track('c')]
    pinned = next(o for o in options(profiles[0], profiles[2], limit=18) if o['technique'] == 'cut')
    brief = {'target_seconds': 300, 'tolerance_seconds': 250, 'must_play': ['a', 'b', 'c'],
             'pinned': [{'transition_id': pinned['id']}]}
    result = build_plans(profiles, brief, {pinned['id']: pinned})
    assert result['plans'] and all(pinned['id'] in p['transition_ids'] for p in result['plans'])
    lookup = {o['id']: o for o in result['transitions']}
    assert validate_plan(result['plans'][0], profiles, lookup, result['plans'][0]['brief'])['valid']
    missing = build_plans(profiles, brief, {})
    assert not missing['plans'] and any('Pinned transition' in c for c in missing['conflicts'])


def test_replanning_keeps_the_prefix():
    profiles = [track(i) for i in 'abcd']
    brief = {'target_seconds': 380, 'tolerance_seconds': 300, 'must_play': list('abc'), 'start': 'a'}
    first = build_plans(profiles, brief)['plans'][0]
    lookup = {o['id']: o for o in build_plans(profiles, brief)['transitions']}
    prefix = {'track_ids': first['track_ids'][:2], 'transition_ids': first['transition_ids'][:1]}
    again = build_plans(profiles, {**brief, 'excluded': [first['track_ids'][2]] if first['track_ids'][2] not in brief['must_play'] else []},
                        lookup, prefix)
    assert again['plans']
    for plan in again['plans']:
        assert plan['track_ids'][:2] == prefix['track_ids'] and plan['transition_ids'][:1] == prefix['transition_ids']
        assert 'Keeps the first 2 tracks' in plan['explanation']
    # Unknown kept transitions are recalculated rather than refused.
    rebuilt = build_plans(profiles, brief, {}, prefix)
    assert rebuilt['plans'] and all(p['track_ids'][:2] == prefix['track_ids'] for p in rebuilt['plans'])
    assert 'Recalculated 1 kept transition' in rebuilt['plans'][0]['explanation']
    with pytest.raises(ValueError, match='Kept'):
        build_plans(profiles, brief, {}, {'track_ids': ['a', 'a'], 'transition_ids': ['x']})


def test_review_queue_ranks_disagreement_and_stale_cues_first():
    calm = track('calm')
    calm['cues'] = [{'id': 'c1', 'role': 'B', 'review': 'proposed'}]
    busy = track('busy')
    busy['cues'] = [{'id': 'c2', 'role': 'D', 'review': 'stale'}]
    busy['observations'] = [{'event': 'outro', 'bar': 40, 'state': 'estimated', 'source': 'energy-baseline'},
                            {'event': 'outro', 'bar': None, 'state': 'absent', 'source': 'gemini'}]
    done = track('done')
    done['cues'] = [{'id': 'c3', 'role': 'D', 'review': 'approved'}]
    rows = queue([calm, busy, done], sets=[{'track_ids': ['busy']}])
    assert [r['track_id'] for r in rows] == ['busy', 'calm']
    assert any('disagree' in r for r in rows[0]['reasons']) and any('saved set' in r for r in rows[0]['reasons'])
    assert len(queue([calm, busy, done], include_done=True)) == 3


def test_path_remapping_is_prefix_and_case_safe():
    rules = portable.parse_rules([{'from': 'D:/Music', 'to': 'C:/Library'}, {'from': 'D:/Music/Deep', 'to': 'E:/Deep'}])
    assert portable.remap('d:\\music\\House\\x.wav', rules).replace('\\', '/') == 'C:/Library/House/x.wav'
    assert portable.remap('D:/Music/Deep/y.wav', rules).replace('\\', '/') == 'E:/Deep/y.wav'
    assert portable.remap('D:/Musical/z.wav', rules) == 'D:/Musical/z.wav'
    with pytest.raises(ValueError):
        portable.parse_rules([{'from': '', 'to': 'x'}])


@pytest.mark.parametrize('kind,suffix', [('workspace', '.json'), ('workspace-backup', '.db')])
def test_portable_workspace_import_remaps_and_rechecks_audio(tmp_path, song, kind, suffix):
    from onset_service.service import Service
    old = tmp_path / 'old-machine' / 'Music'
    old.mkdir(parents=True)
    shutil.copyfile(song, old / 'set.wav')
    source = Service(tmp_path / 'source')
    profile = source.store.import_track(old / 'set.wav')
    source.store.apply_analysis(profile['id'], analysis(old / 'set.wav'))
    source.store.edit(profile['id'], {'mood_tags': ['warm']})
    exported = tmp_path / ('export' + suffix)
    source.export({'path': str(exported), 'kind': kind})
    source.close()
    new = tmp_path / 'new-machine' / 'Library'
    new.mkdir(parents=True)
    shutil.copyfile(old / 'set.wav', new / 'set.wav')
    shutil.rmtree(old)
    target = Service(tmp_path / 'target')
    report = target.import_workspace({'path': str(exported), 'remap': [{'from': str(old), 'to': str(new)}]})
    assert report['located'] == 1 and not report['missing_audio']
    imported = target.store.get(profile['id'])
    assert imported['path'] == str((new / 'set.wav').resolve()) and imported['analysis']
    if suffix == '.db':
        assert report['history_rows'] >= 1
    target.close()
    unmapped = Service(tmp_path / 'unmapped')
    assert unmapped.import_workspace({'path': str(exported)})['missing_audio'] == [profile['id']]
    unmapped.close()


def test_cache_relocation_rewrites_feature_and_stem_paths(tmp_path):
    from onset_service.store import Store
    old, new = tmp_path / 'cache-old', tmp_path / 'cache-new'
    (old / 'k').mkdir(parents=True)
    (old / 'k' / 'f.npz').write_bytes(b'x')
    (old / 'k' / 'vocals.wav').write_bytes(b'y')
    (old / 'k' / 'result.json').write_text(json.dumps({'features': {'path': str(old / 'k' / 'f.npz')}}))
    store = Store(tmp_path / 'app')
    audio = tmp_path / 'a.wav'
    audio.write_bytes(b'audio')
    p = store.import_track(audio)
    p['analysis'] = {'features': {'path': str(old / 'k' / 'f.npz')}, 'deep': {'stems': {'vocals': str(old / 'k' / 'vocals.wav')}}}
    store.save(p)
    with pytest.raises(ValueError, match='Move the cache'):
        portable.relocate_cache(store, str(old), str(new))
    shutil.move(str(old), str(new))
    result = portable.relocate_cache(store, str(old), str(new))
    assert result['updated'] == 1 and result['cache_files_rewritten'] == 1
    moved = store.get(p['id'])['analysis']
    assert moved['features']['path'] == str(new / 'k' / 'f.npz') and moved['deep']['stems']['vocals'] == str(new / 'k' / 'vocals.wav')
    assert json.loads((new / 'k' / 'result.json').read_text())['features']['path'] == str(new / 'k' / 'f.npz')
    store.close()


def test_plan_job_completes_and_can_be_cancelled(tmp_path, song, monkeypatch):
    from onset_service.service import Service
    import onset_service.planning as planning
    service = Service(tmp_path / 'app')
    ids = []
    for i in range(3):
        path = tmp_path / f't{i}.wav'
        path.write_bytes(song.read_bytes() + bytes([i]))
        p = service.store.import_track(path)
        p.update({k: v for k, v in track(p['id']).items() if k != 'id'})
        service.store.save(p)
        ids.append(p['id'])
    brief = {'track_ids': ids, 'target_seconds': 300, 'tolerance_seconds': 250}
    job = service.dispatch('plan-job', {'brief': brief})
    service.pool.submit(lambda: None).result()
    for _ in range(200):
        record = next(j for j in service.store.jobs() if j['id'] == job['id'])
        if record['state'] not in {'queued', 'running'}:
            break
        threading.Event().wait(.05)
    assert record['state'] == 'complete' and record['result']['plan_ids']
    assert service.store.artifact('set', record['result']['plan_ids'][0])
    started, release = threading.Event(), threading.Event()
    original = planning.options
    def slow(*a, **k):
        started.set()
        release.wait(5)
        return original(*a, **k)
    monkeypatch.setattr(planning, 'options', slow)
    job = service.dispatch('plan-job', {'brief': {**brief, 'mood': 'other'}})
    assert started.wait(5)
    service.job_control(job['id'], 'cancel')
    release.set()
    for _ in range(200):
        record = next(j for j in service.store.jobs() if j['id'] == job['id'])
        if record['state'] not in {'queued', 'running', 'cancelling'}:
            break
        threading.Event().wait(.05)
    assert record['state'] == 'cancelled'
    service.close()


def test_resolution_change_reuses_completed_deep_models(tmp_path, song, monkeypatch):
    import onset_service.deep as deep
    from onset_service.analysis import analyse
    from onset_service.store import Store
    store = Store(tmp_path / 'app')
    p = store.import_track(song)
    stem = tmp_path / 'vocals.wav'
    calls = []
    def completed(*args, **kwargs):
        calls.append(1)
        import soundfile as sf
        sf.write(stem, np.zeros((22050 * 24, 1)), 22050)
        return {'models': {name: {'state': 'complete', 'data': {}} for name in ('beat_this', 'demucs', 'essentia', 'genre')},
                'stems': {'vocals': str(stem)}}
    monkeypatch.setattr(deep, 'run_deep', completed)
    first = analyse(p, store.home, deep=True)
    assert 'mix_windows' in first['structure'] and first['vocal_activity'] is not None
    result = next((store.home / 'features').glob('*/result.json'))
    old = json.loads(result.read_text())
    old['resolution_version'] = 'structure-resolution-2'
    result.write_text(json.dumps(old))
    again = analyse(p, store.home, deep=True)
    assert again['resolution_version'] == 'structure-resolution-3' and len(calls) == 1
    (store.home / 'features').joinpath(result.parent.name, 'deep-results.json').unlink()
    old['resolution_version'] = 'structure-resolution-2'
    result.write_text(json.dumps(old))
    assert analyse(p, store.home, deep=True)['state'] == 'complete' and len(calls) == 1
    store.close()


def test_replan_recalculates_kept_transitions_after_a_key_shift():
    profiles = [track(i) for i in 'abcd']
    profiles[1]['key'] = 'C# major'
    brief = {'target_seconds': 380, 'tolerance_seconds': 300, 'must_play': list('abc'), 'start': 'a'}
    first_run = build_plans(profiles, brief)
    first = first_run['plans'][0]
    lookup = {o['id']: o for o in first_run['transitions']}
    prefix = {'track_ids': first['track_ids'][:2], 'transition_ids': first['transition_ids'][:1]}
    shifted = {**brief, 'transpose': {first['track_ids'][1]: -1}}
    again = build_plans(profiles, shifted, lookup, prefix)
    assert again['plans']
    plan = again['plans'][0]
    kept = {o['id']: o for o in again['transitions']}[plan['transition_ids'][0]]
    assert plan['track_ids'][:2] == prefix['track_ids'] and kept['in_transpose'] == -1
    assert plan['transition_ids'][0] != prefix['transition_ids'][0]
    assert 'Recalculated 1 kept transition' in plan['explanation']
    assert validate_plan(plan, profiles, {o['id']: o for o in again['transitions']}, plan['brief'])['valid']


def test_set_search_reports_progress_and_cancels_within_the_first_pass():
    profiles = [track(str(i)) for i in range(8)]
    seen = []
    build_plans(profiles, {'target_seconds': 360, 'tolerance_seconds': 300}, cancelled=lambda progress=None: seen.append(progress))
    fractions = [p for p in seen if p is not None]
    assert len(seen) > 50 and fractions and max(fractions) > .3 and all(0 <= p <= .95 for p in fractions)
    calls = []
    class Stop(Exception):
        pass
    def cancel_soon(progress=None):
        calls.append(progress)
        if len(calls) > 5:
            raise Stop()
    with pytest.raises(Stop):
        build_plans(profiles, {'target_seconds': 360, 'tolerance_seconds': 300}, cancelled=cancel_soon)
    assert max(p or 0 for p in calls) < 1 / 8  # stopped inside the first depth


def test_genre_estimates_split_discogs_labels():
    from onset_service.analysis import genre_estimates
    deep = {'models': {'genre': {'state': 'complete', 'data': {'model': 'discogs-effnet-bsdynamic-1', 'styles': [
        {'label': 'Electronic---Tech House', 'score': .61}, {'label': 'Pop', 'score': .1}]}}}}
    assert genre_estimates(deep) == [
        {'genre': 'Tech House', 'parent': 'Electronic', 'score': .61, 'source': 'discogs-effnet-bsdynamic-1', 'confidence': 'unverified'},
        {'genre': 'Pop', 'parent': None, 'score': .1, 'source': 'discogs-effnet-bsdynamic-1', 'confidence': 'unverified'}]
    assert genre_estimates({}) == []


def test_genre_suggestions_learn_from_your_tags_only(tmp_path):
    from onset_service.genre import suggest
    rng = np.random.default_rng(0)
    centres = {'House': rng.normal(size=1280), 'Techno': rng.normal(size=1280)}
    tracks = []
    def add(i, centre, tags=None, imported=None):
        path = tmp_path / f'{i}.npy'
        np.save(path, centres[centre] + rng.normal(scale=.3, size=1280))
        tracks.append({'id': i, 'genres': imported or tags or [], 'overrides': {'genres': tags} if tags else {},
                       'analysis': {'deep': {'models': {'genre': {'data': {'embedding': str(path)}}}}}})
    for n in range(3):
        add(f'h{n}', 'House', ['House'])
        add(f't{n}', 'Techno', ['Techno'])
    add('new-house', 'House')
    add('new-techno', 'Techno', imported=['Imported Tag'])
    result = suggest(tracks)
    assert result['tagged_tracks'] == 6 and result['spaces'] == {'discogs-effnet': 8}
    best = {r['track_id']: r['suggestions'][0] for r in result['suggestions'] if r['suggestions']}
    assert best['new-house']['genre'] == 'House' and set(best['new-house']['examples']) <= {'h0', 'h1', 'h2'}
    assert best['new-techno']['genre'] == 'Techno'
    assert {r['track_id'] for r in result['suggestions']} == {'new-house', 'new-techno'}
    assert suggest(tracks, {'new-house'})['suggestions'][0]['track_id'] == 'new-house'


def test_genre_suggestions_fall_back_to_native_features_without_mixing_spaces(tmp_path):
    from onset_service.genre import suggest, vector
    rng = np.random.default_rng(1)
    tracks = []
    for n, (label, shift) in enumerate([('House', 0.), ('House', 0.), ('Techno', 3.), ('Techno', 3.), (None, 3.)]):
        path = tmp_path / f'f{n}.npz'
        np.savez(path, mfcc=rng.normal(shift, 1, (40, 13)), rhythm_pattern=rng.normal(shift, 1, (40, 16)),
                 **{k: rng.normal(shift, 1, 40) for k in ('brightness', 'flatness', 'flux', 'percussive', 'low_transient', 'sustained_bass')})
        tracks.append({'id': f'n{n}', 'overrides': {'genres': [label]} if label else {}, 'analysis': {'features': {'path': str(path)}}})
    assert vector(tracks[0])[0] == 'native-timbre' and vector({'id': 'x'}) is None
    result = suggest(tracks)
    assert result['spaces'] == {'native-timbre': 5}
    assert result['suggestions'][0]['suggestions'][0]['genre'] == 'Techno'
