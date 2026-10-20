import {describe,expect,it} from 'vitest';
import {upsertJob} from './jobs';

describe('upsertJob',()=>{
  const jobs=[{id:'a',state:'complete'},{id:'b',state:'running'},{id:'c',state:'cancelled'}];
  it('updates a running job where it is, so the rows under the pointer stay put',()=>{
    expect(upsertJob(jobs,{id:'b',state:'paused'}).map(j=>j.id+':'+j.state)).toEqual(['a:complete','b:paused','c:cancelled']);
  });
  it('puts a new job on top',()=>{
    expect(upsertJob(jobs,{id:'d',state:'queued'}).map(j=>j.id)).toEqual(['d','a','b','c']);
  });
});
