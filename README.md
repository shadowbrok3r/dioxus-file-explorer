okay that part is working pretty good now. i want to add more info to my preview pane for the ai pictures. i want to see all tags, and every single bit of information that the ai has stored about each image / video. also, theres something weird about whats in the database. it looks like none of the descriptions, tags, captions, or ANY metadata in the database. also, do not ever write a string like the one in #sym:generate_vision_description line 82. why would we do that EVER? we are using rust for god sake. we want clear, defined structure. we should be returning the VisionDescription. so fix that, make sure tags are being generated, and that EVERYTHING is being stored in the database. heres what is in there now, which is an unclean, disgusting mess of nested arrays and strings, instead of having actual structure like we should have it:
```
file_explorer/ai_search> return (select * from file_documents ).len()
[6]

file_explorer/ai_search> select * from file_documents
[[{ chunks: [[{ end: 248, start: 0 }, [0]]], id: file_documents:u'0198d8fa-0daa-7181-b426-18d8ffc80cbe', object: { body: 'FILE_PATH:C:\\Users\\Owner\\Pictures\\Screenshots\\Screenshot 2025-08-16 123006.png
HASH:e0f23c61882f40539c88e4f12bd0ee2dc06cd312fd3a5841cd0e1f95409abb04
FILE_TYPE:image
FILE_SIZE:16611
TAGS:
SEGMENTS:
DESCRIPTION:
OCR:

Screenshot 2025-08-16 123006.png', title: 'Screenshot 2025-08-16 123006.png' } }, { chunks: [[{ end: 248, start: 0 }, [1]]], id: file_documents:u'0198d8fa-0de9-7621-b291-c3966974ce51', object: { body: 'FILE_PATH:C:\\Users\\Owner\\Pictures\\Screenshots\\Screenshot 2025-08-16 131851.png
HASH:a583e2e479aa4f9966ed1becf7c5fba51e4dea3fb74c75733d807e46f4825ab7
FILE_TYPE:image
FILE_SIZE:16783
TAGS:
SEGMENTS:
DESCRIPTION:
OCR:

Screenshot 2025-08-16 131851.png', title: 'Screenshot 2025-08-16 131851.png' } }, { chunks: [[{ end: 248, start: 0 }, [2]]], id: file_documents:u'0198d8fa-0e28-77e1-a4f9-ff75e311cce7', object: { body: 'FILE_PATH:C:\\Users\\Owner\\Pictures\\Screenshots\\Screenshot 2025-08-19 080246.png
HASH:8d0a49df308f119a101d039a642fdfd5217a8afc99882d77dfad020b9b024128
FILE_TYPE:image
FILE_SIZE:25145
TAGS:
SEGMENTS:
DESCRIPTION:
OCR:

Screenshot 2025-08-19 080246.png', title: 'Screenshot 2025-08-19 080246.png' } }, { chunks: [[{ end: 248, start: 0 }, [3]]], id: file_documents:u'0198d8fa-0e67-7992-91cd-9ab9d860ea64', object: { body: 'FILE_PATH:C:\\Users\\Owner\\Pictures\\Screenshots\\Screenshot 2025-08-20 122305.png
HASH:a43021a503a25c5f357749affc8a98e67c8d1db33007f3f04323357a39892a6a
FILE_TYPE:image
FILE_SIZE:84549
TAGS:
SEGMENTS:
DESCRIPTION:
OCR:

Screenshot 2025-08-20 122305.png', title: 'Screenshot 2025-08-20 122305.png' } }, { chunks: [[{ end: 248, start: 0 }, [4]]], id: file_documents:u'0198d8fa-0ea2-7903-b2a3-baf752d7b734', object: { body: 'FILE_PATH:C:\\Users\\Owner\\Pictures\\Screenshots\\Screenshot 2025-08-20 135926.png
HASH:8643a33d4be36440e92b6f0ccbdac1d72ec3e1a1af28382a92d9fc8275b15cb9
FILE_TYPE:image
FILE_SIZE:16343
TAGS:
SEGMENTS:
DESCRIPTION:
OCR:

Screenshot 2025-08-20 135926.png', title: 'Screenshot 2025-08-20 135926.png' } }, { chunks: [[{ end: 248, start: 0 }, [5]]], id: file_documents:u'0198d8fa-0ede-7e63-8921-fc6ea11a83ae', object: { body: 'FILE_PATH:C:\\Users\\Owner\\Pictures\\Screenshots\\Screenshot 2025-08-23 134400.png
HASH:1232f88406fa5075334bd4083b23dc06833eb10025594af1b79a91cf3b057e5a
FILE_TYPE:image
FILE_SIZE:52723
TAGS:
SEGMENTS:
DESCRIPTION:
OCR:

Screenshot 2025-08-23 134400.png', title: 'Screenshot 2025-08-23 134400.png' } }]]
```