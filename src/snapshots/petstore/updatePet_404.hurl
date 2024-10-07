PUT {{ baseurl }}/pet
Authorization: Bearer {{ authorization }}
Prefer: code=404
{
  "category": {
    "id": 0,
    "name": ""
  },
  "id": 0,
  "name": "",
  "photoUrls": [
    ""
  ],
  "status": "",
  "tags": [
    {
      "id": 0,
      "name": ""
    }
  ]
}
HTTP 404
