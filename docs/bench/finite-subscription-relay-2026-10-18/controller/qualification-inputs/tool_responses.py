"""Authored offline Responses script. No socket, process or model dispatch.

This tests transport and actual returned tool results, never model quality.
The native Runtime receipt remains the sole authority for admission/cleanup.
"""
import copy
import json

TOOLS = frozenset('mcp__finite__'+name for name in ('read_file','write_file','exec'))
MAX_REQUEST = 262144


def _catalog(request):
    found = {}

    def walk(entries, namespace=None):
        if not isinstance(entries, list) or len(entries)>32:
            raise ValueError('tool_catalog_bound')
        for item in entries:
            if not isinstance(item, dict):raise ValueError('tool_schema')
            if item.get('type')=='namespace' and namespace is None:
                if item.get('name')!='mcp__finite':raise ValueError('unexpected_namespace')
                walk(item.get('tools'), item.get('name'))
                continue
            wire_name = item.get('name')
            name = 'mcp__finite__'+wire_name if namespace=='mcp__finite' and isinstance(wire_name,str) else wire_name
            if name not in TOOLS or name in found or item.get('type')!='function':
                raise ValueError('unexpected_tool')
            found[name] = (namespace,wire_name)

    walk(request.get('tools', []))
    for item in request.get('input', []):
        if item.get('type')=='additional_tools':walk(item.get('tools', []))
    if set(found)!=TOOLS:raise ValueError('missing_tool_schema')
    return found


def _result(value, depth=0):
    if depth>8:raise ValueError('tool_result_depth')
    if isinstance(value, str):
        try:value=json.loads(value)
        except (ValueError, TypeError):raise ValueError('tool_result_json') from None
        return _result(value, depth+1)
    if isinstance(value, list):
        if len(value)!=1 or not isinstance(value[0], dict):raise ValueError('tool_result_content')
        item=value[0]
        if item.get('type') not in {'text','input_text'}:raise ValueError('tool_result_content')
        return _result(item.get('text'), depth+1)
    if not isinstance(value, dict):raise ValueError('tool_result_shape')
    if 'isError' in value and value['isError'] is not False:raise ValueError('mcp_error')
    if 'structuredContent' in value:return _result(value['structuredContent'], depth+1)
    if 'content' in value:return _result(value['content'], depth+1)
    return value


def _response(model, index, tool=None, arguments=None, namespace=None):
    events=[]

    def emit(kind, **fields):
        events.append('event: '+kind+'\ndata: '+json.dumps({'type':kind, **fields})+'\n\n')

    response={'id':f'resp_r2_local_{index}', 'object':'response', 'created_at':1,
              'status':'in_progress', 'model':model, 'output':[]}
    emit('response.created', response=response)
    if tool is not None:
        text=json.dumps(arguments, separators=(',',':'))
        item={'id':f'fc_r2_local_{index}', 'call_id':f'call_r2_local_{index}',
              'type':'function_call', 'name':tool}
        if namespace is not None:item['namespace']=namespace
        emit('response.output_item.added', output_index=0, item={**item, 'arguments':''})
        emit('response.function_call_arguments.delta', item_id=item['id'], output_index=0, delta=text)
        emit('response.function_call_arguments.done', item_id=item['id'], output_index=0, arguments=text)
        item={**item, 'arguments':text, 'status':'completed'}
    else:
        text='Operator-authored local tool loop complete. No model ran.'
        item={'id':f'msg_r2_local_{index}', 'type':'message', 'role':'assistant',
              'status':'in_progress', 'content':[]}
        emit('response.output_item.added', output_index=0, item=item)
        emit('response.content_part.added', item_id=item['id'], output_index=0, content_index=0,
             part={'type':'output_text','text':'','annotations':[]})
        emit('response.output_text.delta', item_id=item['id'], output_index=0, content_index=0, delta=text)
        emit('response.output_text.done', item_id=item['id'], output_index=0, content_index=0, text=text)
        item={**item, 'status':'completed', 'content':[{'type':'output_text','text':text,'annotations':[]}]}
    emit('response.output_item.done', output_index=0, item=item)
    emit('response.completed', response={**response,'status':'completed','output':[item]})
    return ''.join(events).encode()


class ToolScript:
    def __init__(self, steps, *, model='gpt-6-luna', effort='medium', marker='R2-LOCAL-TOOLS-V1'):
        if not isinstance(steps,list) or not 1<=len(steps)<=16:raise ValueError('script_bound')
        for step in steps:
            if (not isinstance(step,dict) or step.get('tool') not in TOOLS
                    or not isinstance(step.get('arguments'),dict)
                    or not isinstance(step.get('expected'),dict)):
                raise ValueError('script_shape')
        self.steps = copy.deepcopy(steps)
        self.model, self.effort, self.marker = model, effort, marker
        self.index = 0
        self.observed_tool_results = 0

    def respond(self, request):
        if self.index>len(self.steps):raise ValueError('script_exhausted')
        if not isinstance(request,dict) or len(json.dumps(request).encode())>MAX_REQUEST:
            raise ValueError('request_bound')
        if (request.get('model')!=self.model or request.get('reasoning',{}).get('effort')!=self.effort
                or request.get('stream') is not True or request.get('store',False) is not False):
            raise ValueError('request_identity')
        entries=request.get('input')
        if not isinstance(entries,list) or len(entries)>256 or not all(isinstance(i,dict) for i in entries):
            raise ValueError('request_input')
        texts=[]
        for item in entries:
            if item.get('role')!='user':continue
            content=item.get('content')
            if isinstance(content,str):texts.append(content)
            elif isinstance(content,list):
                texts.extend(p['text'] for p in content if isinstance(p,dict)
                    and p.get('type') in {'text','input_text'} and isinstance(p.get('text'),str))
        if not any(self.marker in text for text in texts):raise ValueError('public_marker_missing')
        catalog=_catalog(request)
        if self.index:
            expected_call=f'call_r2_local_{self.index}'
            outputs=[i for i in entries if i.get('type')=='function_call_output'
                     and i.get('call_id')==expected_call]
            if not outputs:raise ValueError('missing_tool_result')
            if len(outputs)!=1:raise ValueError('duplicate_tool_result')
            result=_result(outputs[0].get('output'))
            previous=self.steps[self.index-1]
            for key,value in previous['expected'].items():
                if key not in result or type(result[key]) is not type(value) or result[key]!=value:
                    raise ValueError('tool_result_mismatch')
            if previous.get('stdout_contains') not in (None,''):
                if previous['stdout_contains'] not in result.get('stdout',''):
                    raise ValueError('tool_result_stdout')
        if self.index==len(self.steps):
            response=_response(self.model,self.index+1)
        else:
            step=self.steps[self.index]
            namespace,wire_name=catalog[step['tool']]
            response=_response(self.model,self.index+1,wire_name,step['arguments'],namespace)
        self.observed_tool_results=self.index
        self.index+=1
        return response
