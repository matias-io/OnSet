"""Resource observations are diagnostic metadata, never analysis identity."""
from functools import wraps
import threading,time

def measure_resources(function):
    @wraps(function)
    def measured(*args,**kwargs):
        import psutil
        process=psutil.Process();stop=threading.Event();peak=[process.memory_info().rss];start=time.monotonic()
        def sample():
            while not stop.wait(.1):
                try:peak[0]=max(peak[0],process.memory_info().rss)
                except psutil.Error:break
        thread=threading.Thread(target=sample,daemon=True);thread.start()
        try:
            result=function(*args,**kwargs)
            result['resources']={'elapsed_seconds':time.monotonic()-start,'peak_process_rss_bytes':max(peak[0],process.memory_info().rss),
                                 'scope':'Native process; concurrent native jobs share this measurement. WSL models report separately.'}
            return result
        finally:stop.set();thread.join(timeout=1)
    return measured

def semantic_observation(value):
    if isinstance(value,dict):return {k:semantic_observation(v) for k,v in value.items() if k not in {'seconds','resources','cache_hit'}}
    if isinstance(value,list):return [semantic_observation(v) for v in value]
    return value
