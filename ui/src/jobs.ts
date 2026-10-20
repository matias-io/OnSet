/** A job's progress updates its row where it is; only a new job goes on top. Moving the row
 * on every update shifted the list under the pointer, so a click meant for one job's Resume
 * or Cancel landed on another's (one click cancelled three analysis jobs). */
export function upsertJob<T extends {id:string}>(jobs:T[],job:T):T[]{
  return jobs.some(j=>j.id===job.id)?jobs.map(j=>j.id===job.id?job:j):[job,...jobs];
}
