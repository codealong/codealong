FROM docker.elastic.co/kibana/kibana-oss:6.6.0

USER 0

RUN yum -y install epel-release && yum -y update && yum clean all
RUN yum -y install nodejs
RUN npm install -g elasticdump

COPY bin /user/local/codealong/bin
COPY data /user/local/codealong/data

USER 1000

CMD ["/user/local/codealong/bin/kibana-codealong"]
