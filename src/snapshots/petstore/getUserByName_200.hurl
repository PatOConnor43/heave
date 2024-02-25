GET {{ baseurl }}/user/{{username}}
Authorization: Bearer {{ authorization }}
Prefer: code=200

HTTP 200

[Asserts]
jsonpath "$" isCollection
#jsonpath "$.id" isInteger
#jsonpath "$.username" isString
#jsonpath "$.firstName" isString
#jsonpath "$.lastName" isString
#jsonpath "$.email" isString
#jsonpath "$.password" isString
#jsonpath "$.phone" isString
#jsonpath "$.userStatus" isInteger
