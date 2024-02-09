GET {{ baseurl }}/pet/findByTags
Authorization: Bearer {{ authorization }}
Prefer: code=200

[QueryStringParams]
tags: 

HTTP 200

[Asserts]
jsonpath "$" isCollection

