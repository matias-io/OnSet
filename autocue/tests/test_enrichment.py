import copy
import pytest
from autocue_app.service import Service


def test_enrichment_preserves_citation_audit_and_restart(tmp_path, song):
    service = Service(tmp_path/'app', lambda e: None)
    track = service.store.import_track(song)
    record = {'track_id': track['id'], 'research': {'completed': True, 'consulted_sources': ['https://example.org/release']},
              'claims': [{'kind': 'genre', 'value': 'House', 'source_type': 'web', 'url': 'https://example.org/release',
                          'citation_check': {'status': 'retained', 'requested_url': 'https://example.org/release'}}]}
    original = copy.deepcopy(record)
    assert service.enrichment({'schema_version': 1, 'records': [record], 'allowed_ids': [track['id']]}) == {'saved': 1}
    assert record == original
    service.close()
    service = Service(tmp_path/'app', lambda e: None)
    saved = service.evidence({'id': track['id']})['enrichment']
    assert saved['research'] == original['research']
    assert saved['claims'][0]['citation_check'] == original['claims'][0]['citation_check']
    assert saved['claims'][0]['retrieved']
    service.close()


@pytest.mark.parametrize('bad', [
    {'track_id': 'unknown', 'claims': []},
    {'claims': [{'kind': 'bpm', 'value': '128', 'source_type': 'model'}]},
    {'claims': [{'kind': 'genre', 'value': 'House', 'source_type': 'web', 'url': 'https://user:password@example.org'}]},
    {'claims': [{'kind': 'genre', 'value': 'House', 'source_type': 'web', 'url': 'https://'}]},
    {'claims': [{'kind': 'genre', 'value': 'House', 'source_type': 'invented'}]},
])
def test_invalid_enrichment_batch_never_partially_saves(tmp_path, song, bad):
    service = Service(tmp_path/'app', lambda e: None)
    track = service.store.import_track(song)
    first = {'track_id': track['id'], 'claims': [{'kind': 'mood', 'value': 'Driving', 'source_type': 'model'}]}
    other = tmp_path/'other.wav'
    other.write_bytes(song.read_bytes()+b'\0\0')
    second_track = service.store.import_track(other)
    second = {'track_id': second_track['id'], **bad}
    with pytest.raises(ValueError):
        service.enrichment({'schema_version': 1, 'records': [first, second]})
    assert service.store.artifacts('enrichment') == []
    service.close()
